//! Actual pending native cleanup I/O at an isolated private-channel EOF boundary.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use tokio::task::JoinHandle;

use super::{CLEANUP, Diagnostic, Request, enqueue, own_worker, queue, wait_for_reply};
use crate::windows::loader_tests::{FixtureChild, bounded_text};

static NEXT_PROBE: Mutex<Option<Probe>> = Mutex::new(None);
const CONFIRMED: &str = "actual-tree-pending-io-retained";

#[derive(Debug)]
struct Event {
    phase: &'static str,
    at: Instant,
}

pub(super) struct Probe {
    reader: Option<io::PipeReader>,
    read: Option<JoinHandle<io::Result<()>>>,
    events: mpsc::SyncSender<Event>,
}

fn event(events: &mpsc::SyncSender<Event>, phase: &'static str) {
    events
        .try_send(Event {
            phase,
            at: Instant::now(),
        })
        .expect("bounded EOF receipt channel must remain available");
}

pub(super) fn take() -> Option<Probe> {
    NEXT_PROBE.lock().unwrap().take()
}

impl Probe {
    pub(super) fn start(&mut self, deadline: Instant) {
        let Some(mut reader) = self.reader.take() else {
            return;
        };
        self.events
            .try_send(Event {
                phase: "cleanup-entered",
                at: deadline.checked_sub(CLEANUP).unwrap(),
            })
            .expect("bounded EOF entry receipt must remain available");
        let events = self.events.clone();
        self.read = Some(tokio::task::spawn_blocking(move || {
            event(&events, "native-read-entered");
            reader.read_exact(&mut [0_u8; 1])?;
            event(&events, "native-read-complete");
            Ok(())
        }));
    }

    pub(super) async fn confirm_tree_and_read_until(
        &mut self,
        deadline: Instant,
    ) -> io::Result<()> {
        event(&self.events, "root-tree-confirmed");
        let read = self.read.as_mut().expect("actual cleanup read started");
        // A started blocking task survives timeout/abort; this handle stays in the parked owner.
        tokio::time::timeout_at(
            tokio::time::Instant::from_std(deadline),
            crate::windows::adapter_poll_until(deadline, async {
                read.await.map_err(io::Error::other)?
            }),
        )
        .await
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                "fixture native cleanup read remains pending",
            )
        })??;
        Ok(())
    }

    pub(super) fn quarantined(&self) {
        event(&self.events, "owner-quarantined");
    }
}

fn stock_is_held() {
    let error = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(crate::windows::stock_powershell().unwrap())
        .unwrap_err();
    assert_eq!(error.raw_os_error(), Some(32));
}

pub(in crate::windows) fn helper() {
    assert!(super::DISPATCHER.get().is_none());
    assert!(crate::windows::USER_SID.get().is_none());
    let started = Instant::now();
    let deadline = started + Duration::from_secs(40);
    let (reader, mut writer) = io::pipe().unwrap();
    let (events, receipts) = mpsc::sync_channel(8);
    *NEXT_PROBE.lock().unwrap() = Some(Probe {
        reader: Some(reader),
        read: None,
        events,
    });
    let (sender, receiver) = queue::channel(16);
    let owner = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(own_worker(receiver));
    });
    exchange_sid(&sender, started);
    drop(sender);
    let quarantine = verify_quarantine(&receipts, &owner, &mut writer, deadline);
    drop(writer);
    let completed = receipts
        .recv_timeout(crate::windows::remaining(deadline).unwrap())
        .unwrap();
    assert_eq!(completed.phase, "native-read-complete");
    assert!(completed.at > quarantine);
    assert!(!owner.is_finished());
    stock_is_held();
    crate::windows::remaining(deadline).unwrap();
    fs::write("eof-proof-confirmed", CONFIRMED).unwrap();
    // Only the disposable helper's retained parent-owned process handle ends this quarantine.
    loop {
        std::thread::park();
    }
}

fn exchange_sid(sender: &queue::Sender<Request>, started: Instant) {
    let (reply, response) = mpsc::sync_channel(1);
    let request_deadline = started + crate::windows::ADAPTER_TIMEOUT;
    let cancelled = Arc::new(super::AtomicBool::new(false));
    enqueue(
        sender,
        Request {
            id: "1".to_owned(),
            operation: "sid",
            bytes: b"{\"version\":1,\"id\":\"1\",\"operation\":\"sid\",\"path\":null}\n".to_vec(),
            deadline: request_deadline,
            cancelled: Arc::clone(&cancelled),
            reply,
            diagnostic: Arc::new(Diagnostic::new(
                "1".to_owned(),
                "sid",
                started,
                request_deadline,
            )),
        },
    )
    .unwrap();
    let result = wait_for_reply(&response, &cancelled, request_deadline).unwrap();
    assert!(result.as_str().unwrap().starts_with("S-1-"));
}

fn verify_quarantine(
    receipts: &mpsc::Receiver<Event>,
    owner: &std::thread::JoinHandle<()>,
    writer: &mut io::PipeWriter,
    deadline: Instant,
) -> Instant {
    let mut observed = Vec::with_capacity(4);
    while observed
        .last()
        .is_none_or(|receipt: &Event| receipt.phase != "owner-quarantined")
    {
        observed.push(
            receipts
                .recv_timeout(crate::windows::remaining(deadline).unwrap())
                .unwrap(),
        );
        assert!(observed.len() <= 4);
    }
    let entry = observed
        .iter()
        .find(|receipt| receipt.phase == "cleanup-entered")
        .unwrap();
    let root = observed
        .iter()
        .find(|receipt| receipt.phase == "root-tree-confirmed")
        .unwrap();
    let quarantine = observed.last().unwrap();
    assert!(
        observed
            .iter()
            .any(|receipt| receipt.phase == "native-read-entered")
    );
    assert!(root.at < entry.at + CLEANUP);
    assert!(quarantine.at >= entry.at + CLEANUP);
    assert!(!owner.is_finished());
    stock_is_held();
    assert!(matches!(
        receipts.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    writer.write_all(&[1]).unwrap();
    quarantine.at
}

pub(in crate::windows) fn driver() {
    // No SID, dispatcher or StockAdapterGuard call occurs in this fresh exact orchestrator.
    assert!(super::DISPATCHER.get().is_none());
    assert!(crate::windows::USER_SID.get().is_none());
    let deadline = Instant::now() + Duration::from_secs(40);
    let directory = tempfile::tempdir().unwrap();
    let stdout = directory.path().join("stdout");
    let stderr = directory.path().join("stderr");
    let mut child = FixtureChild(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "windows::loader_tests::owned_loader_fixture_child",
                "--nocapture",
            ])
            .env("LOCRON_STOCK_LOADER_FIXTURE", "eof-helper")
            .current_dir(directory.path())
            .creation_flags(0x0800_0000)
            .stdin(Stdio::null())
            .stdout(File::create(&stdout).unwrap())
            .stderr(File::create(&stderr).unwrap())
            .spawn()
            .unwrap(),
    );
    let receipt = directory.path().join("eof-proof-confirmed");
    loop {
        crate::windows::remaining(deadline).unwrap_or_else(|error| {
            panic!(
                "{error}: stdout={} stderr={}",
                bounded_text(&stdout),
                bounded_text(&stderr)
            );
        });
        let status = child.0.try_wait().unwrap();
        assert!(
            status.is_none(),
            "EOF helper exited {status:?}: stderr={}",
            bounded_text(&stderr)
        );
        if receipt.is_file() {
            assert_eq!(bounded_text(&receipt), CONFIRMED);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    stock_is_held();
    child.0.kill().unwrap();
    loop {
        crate::windows::remaining(deadline).unwrap();
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(!status.success());
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let released = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(crate::windows::stock_powershell().unwrap())
        .unwrap();
    crate::windows::remaining(deadline).unwrap();
    drop(released);
    println!("private-worker-eof-retained-io-confirmed");
}

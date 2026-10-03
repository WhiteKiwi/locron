//! Post-cold diagnostics use the actual guarded adapter, its limits and exit proof.

use std::sync::Arc;
use std::time::Instant;

use serde_json::json;

use super::{ADAPTER_TIMEOUT, ADAPTER_WORKERS, prepare_adapter, run_adapter_worker};

const BOOTSTRAP: &str = include_str!("loader_diagnostic_bootstrap.ps1");
const STRUCTURED: &str = include_str!("loader_diagnostic_structured.ps1");
const ECHO: &str = "ARM64 한글 δοκιμή";

pub(super) fn driver(mode: &str) {
    let helper = match mode {
        "diagnostic-bootstrap" => "diagnostic-bootstrap-helper",
        "diagnostic-small" => "diagnostic-small-helper",
        "diagnostic-60k" => "diagnostic-60k-helper",
        _ => panic!("unknown guarded diagnostic driver"),
    };
    super::loader_tests::isolated(helper, "guarded-stock-diagnostic-confirmed");
}

pub(super) fn probe(mode: &str) {
    let start = Instant::now();
    let deadline = start + ADAPTER_TIMEOUT;
    let (payload, expected_bytes, script) = match mode {
        "diagnostic-bootstrap-helper" => (String::new(), 0, BOOTSTRAP),
        "diagnostic-small-helper" => ("small".to_owned(), 54, STRUCTURED),
        "diagnostic-60k-helper" => ("x".repeat(60_000), 60_049, STRUCTURED),
        _ => panic!("unknown guarded diagnostic probe"),
    };
    let mut request = prepare_adapter(script, &json!({"echo":ECHO,"payload":payload})).unwrap();
    if expected_bytes == 0 {
        // Genuine zero stdin bytes: the real loader/EOF/retained parser still executes unchanged.
        request.input.clear();
    }
    assert_eq!(request.input.len(), expected_bytes);
    let trace = Arc::clone(&request.trace);
    let permit = ADAPTER_WORKERS.acquire(deadline).unwrap();
    let result = run_adapter_worker(request, deadline, permit).unwrap_or_else(|error| {
        trace.report("diagnostic-failed");
        panic!("guarded diagnostic {mode} failed: {error}")
    });
    super::remaining(deadline).unwrap();
    assert_eq!(
        result["input_bytes"].as_u64(),
        Some(u64::try_from(expected_bytes).unwrap())
    );
    assert_eq!(
        result["payload_length"].as_u64(),
        Some(u64::try_from(payload.len()).unwrap())
    );
    if expected_bytes == 0 {
        assert!(result["echo"].is_null());
        assert_eq!(result["stage"], "entered");
    } else {
        assert_eq!(result["echo"], ECHO);
        assert_eq!(result["stage"], "stdin-parsed");
    }
    let sid = result["sid"].as_str().unwrap();
    assert!(
        sid.starts_with("S-1-")
            && sid
                .split('-')
                .skip(1)
                .all(|part| part.parse::<u64>().is_ok())
    );
    assert!(result["ps_version"].as_str().unwrap().starts_with("5.1."));
    assert_eq!(result["is64bit"], true);
    let (machine, architecture) = if cfg!(target_arch = "aarch64") {
        (0xaa64, "ARM64")
    } else {
        assert!(cfg!(target_arch = "x86_64"));
        (0x8664, "AMD64")
    };
    assert_eq!(result["machine"], machine);
    assert_eq!(result["process_architecture"], architecture);
    let pid = trace.stage_value("spawn-complete").unwrap();
    assert!(pid > 0);
    assert_eq!(result["pid"].as_u64(), Some(u64::from(pid)));
    assert_eq!(trace.stage_value("spawn-flags"), Some(0x0800_0004));
    assert!(trace.has_stage("input-written") && trace.has_stage("root-completed"));
    assert_eq!(
        trace.child_phases(),
        [
            "source-entry",
            "encoding-ready",
            "binding-start",
            "binding-ready",
            "input-complete",
            "json-start",
            "json-parsed",
            "caller-start",
            "caller-complete"
        ]
    );
    // root-completed is recorded only after root+Job emptiness and all three pipe tasks finish.
    println!(
        "guarded-stock-diagnostic-confirmed {}",
        json!({
            "diagnostic":mode, "pid":pid, "input_bytes":expected_bytes,
            "elapsed_ms":start.elapsed().as_millis(), "cleanup_confirmed":true, "child_facts":result,
        })
    );
}

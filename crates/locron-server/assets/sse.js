// EventSource wrapper for the run stream: cookie-authenticated (no header
// possible), typed named events, auto-reconnecting, with a "stream ended"
// notice after the terminal `termination` event.
"use strict";

const RunStream = (() => {
  const EVENT_NAMES = ["run", "attempt", "output", "termination"];

  function open(runId, handlers) {
    const source = new EventSource(`/api/v1/runs/${encodeURIComponent(runId)}/stream`);
    const listeners = {};
    for (const name of EVENT_NAMES) listeners[name] = [];
    let ended = false;
    const seenOutput = new Set();

    for (const name of EVENT_NAMES) {
      source.addEventListener(name, (event) => {
        let data;
        try {
          data = JSON.parse(event.data);
        } catch {
          return;
        }
        if (name === "output") {
          const key = `${data.attempt_number}:${data.seq}`;
          if (seenOutput.has(key)) return;
          seenOutput.add(key);
        }
        for (const listener of listeners[name]) listener(data);
        if (name === "termination") {
          ended = true;
          if (handlers.onEnd) handlers.onEnd(data);
        }
      });
    }
    source.onopen = () => {
      if (handlers.onOpen) handlers.onOpen();
    };
    source.onerror = () => {
      if (handlers.onError) handlers.onError(ended);
    };

    return {
      on(name, listener) {
        if (listeners[name]) listeners[name].push(listener);
        return this;
      },
      close() {
        source.close();
      },
    };
  }

  return { open };
})();

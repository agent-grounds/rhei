// What consumes a run's events: the sink every frontend implements, the tee
// that fans one stream out to several, and the sink that drops them.
//
// Its own part because the records say what happened and the sinks decide who
// hears it; the two change for different reasons.

// §AR-source-file-size.3

use std::sync::Arc;

use super::event::RunEvent;

/// Sink that consumes `RunEvent`s. Implementations must be cheap to clone and
/// safe to share across threads (the engine spawns parallel workers).
pub trait EventSink: Send + Sync {
    fn emit(&self, event: RunEvent);
}

/// Composite sink that forwards every event to each inner sink in order.
#[derive(Clone)]
pub struct Tee {
    inners: Arc<Vec<Arc<dyn EventSink>>>,
}

impl Tee {
    pub fn new(sinks: Vec<Arc<dyn EventSink>>) -> Self {
        Self { inners: Arc::new(sinks) }
    }
}

impl EventSink for Tee {
    fn emit(&self, event: RunEvent) {
        for sink in self.inners.iter() {
            sink.emit(event.clone());
        }
    }
}

/// Sink that discards every event. Useful as the default frontend when the
/// engine is responsible for producing stdout (backward-compatible mode).
pub struct NullSink;

impl EventSink for NullSink {
    fn emit(&self, _event: RunEvent) {}
}

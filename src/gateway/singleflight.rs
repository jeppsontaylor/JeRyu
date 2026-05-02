//! Owner: Cache Gateway subsystem — singleflight deduplication
//! Proof: `cargo nextest run -p jeryu -- gateway::singleflight`
//! Invariants: Equivalent in-flight requests coalesce without widening authority or hiding failed producers.
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use tokio::sync::broadcast;

/// A simple singleflight deduplicator for async fetch requests.
/// If multiple tasks request the same key, only the first task proceeds,
/// while subsequent tasks wait for the broadcasted result.
pub struct Singleflight<T: Clone + Send + Sync + 'static> {
    active_requests: DashMap<String, broadcast::Sender<T>>,
}

impl<T: Clone + Send + Sync + 'static> Singleflight<T> {
    pub fn new() -> Self {
        Self {
            active_requests: DashMap::new(),
        }
    }

    /// Attempts to join an active fetch request for `key`.
    /// Returns `Some(Receiver)` if a task is already processing it.
    /// Returns `None` if the caller should proceed to fetch it.
    pub fn join_or_start(&self, key: &str) -> Option<broadcast::Receiver<T>> {
        match self.active_requests.entry(key.to_string()) {
            Entry::Occupied(entry) => Some(entry.get().subscribe()),
            Entry::Vacant(entry) => {
                let (tx, _) = broadcast::channel(1);
                entry.insert(tx);
                None
            }
        }
    }

    /// Marks the fetch as completed and broadcasts the result to all waiting subscribers.
    pub fn complete(&self, key: &str, result: T) {
        if let Some((_, tx)) = self.active_requests.remove(key) {
            let _ = tx.send(result);
        }
    }

    /// Marks the fetch as failed, abandoning waiting subscriber channels.
    /// Future requests for this key will attempt to fetch again.
    pub fn fail(&self, key: &str) {
        self.active_requests.remove(key);
    }
}

impl<T: Clone + Send + Sync + 'static> Default for Singleflight<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// A Drop guard ensuring a panicked elected fetcher clears the singleflight map.
pub struct SingleflightGuard<'a, T: Clone + Send + Sync + 'static> {
    sf: &'a Singleflight<T>,
    key: String,
    completed: bool,
}

impl<'a, T: Clone + Send + Sync + 'static> SingleflightGuard<'a, T> {
    pub fn new(sf: &'a Singleflight<T>, key: &str) -> Self {
        Self {
            sf,
            key: key.to_string(),
            completed: false,
        }
    }
    pub fn complete(mut self, result: T) {
        self.sf.complete(&self.key, result);
        self.completed = true;
    }
}

impl<'a, T: Clone + Send + Sync + 'static> Drop for SingleflightGuard<'a, T> {
    fn drop(&mut self) {
        if !self.completed {
            self.sf.fail(&self.key);
        }
    }
}

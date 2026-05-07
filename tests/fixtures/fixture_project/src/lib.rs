//! # Signal Router
//!
//! A lightweight, zero-allocation signal routing engine designed for high-throughput
//! distributed telemetry pipelines.  Routes signals from heterogeneous producers
//! to typed consumer channels based on configurable filter predicates.

use std::collections::HashMap;

/// Severity levels for incoming signals, ordered from lowest to highest priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Severity {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl Severity {
    /// Returns `true` for severities at or above `Warn`.
    pub fn is_alertable(self) -> bool {
        self >= Severity::Warn
    }
}

/// A single signal event from a telemetry producer.
#[derive(Debug, Clone)]
pub struct Signal {
    pub source: String,
    pub severity: Severity,
    pub payload: String,
    pub timestamp_ms: u64,
}

/// A routing rule that determines which consumer channel receives a signal.
#[derive(Debug, Clone)]
pub struct Route {
    pub name: String,
    pub min_severity: Severity,
    pub source_pattern: Option<String>,
}

impl Route {
    /// Returns `true` if this route matches the given signal.
    pub fn matches(&self, signal: &Signal) -> bool {
        if signal.severity < self.min_severity {
            return false;
        }
        match &self.source_pattern {
            Some(pattern) => signal.source.contains(pattern),
            None => true,
        }
    }
}

/// The core signal router.  Distributes incoming signals to named channels
/// based on registered routing rules.
#[derive(Debug, Default)]
pub struct Router {
    routes: Vec<Route>,
}

impl Router {
    /// Create a new, empty router.
    pub fn new() -> Self {
        Self { routes: Vec::new() }
    }

    /// Register a routing rule.
    pub fn add_route(&mut self, route: Route) {
        self.routes.push(route);
    }

    /// Route a signal to all matching channels.  Returns the list of channel
    /// names that accepted the signal.
    pub fn route(&self, signal: &Signal) -> Vec<&str> {
        self.routes
            .iter()
            .filter(|r| r.matches(signal))
            .map(|r| r.name.as_str())
            .collect()
    }

    /// Bulk-route a batch of signals.  Returns a map from channel name to the
    /// count of signals routed to that channel.
    pub fn route_batch(&self, signals: &[Signal]) -> HashMap<String, usize> {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for signal in signals {
            for channel in self.route(signal) {
                *counts.entry(channel.to_string()).or_default() += 1;
            }
        }
        counts
    }

    /// Returns the number of registered routes.
    pub fn route_count(&self) -> usize {
        self.routes.len()
    }
}

/// Generate a batch of synthetic test signals for benchmarking and validation.
pub fn generate_test_signals(count: usize) -> Vec<Signal> {
    let severities = [
        Severity::Trace,
        Severity::Debug,
        Severity::Info,
        Severity::Warn,
        Severity::Error,
    ];
    let sources = [
        "cache-warmer",
        "telemetry-collector",
        "auth-gateway",
        "job-scheduler",
        "secret-rotator",
    ];

    (0..count)
        .map(|i| Signal {
            source: sources[i % sources.len()].to_string(),
            severity: severities[i % severities.len()],
            payload: format!("event-{i:06}"),
            timestamp_ms: 1_700_000_000_000 + (i as u64 * 100),
        })
        .collect()
}

// ─── Unit Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_signal(source: &str, severity: Severity) -> Signal {
        Signal {
            source: source.to_string(),
            severity,
            payload: "test-payload".to_string(),
            timestamp_ms: 1_700_000_000_000,
        }
    }

    #[test]
    fn severity_ordering_is_correct() {
        assert!(Severity::Trace < Severity::Debug);
        assert!(Severity::Debug < Severity::Info);
        assert!(Severity::Info < Severity::Warn);
        assert!(Severity::Warn < Severity::Error);
        assert!(Severity::Error < Severity::Fatal);
    }

    #[test]
    fn alertable_threshold_is_warn() {
        assert!(!Severity::Trace.is_alertable());
        assert!(!Severity::Info.is_alertable());
        assert!(Severity::Warn.is_alertable());
        assert!(Severity::Error.is_alertable());
        assert!(Severity::Fatal.is_alertable());
    }

    #[test]
    fn route_matches_severity_filter() {
        let route = Route {
            name: "alerts".to_string(),
            min_severity: Severity::Warn,
            source_pattern: None,
        };

        assert!(!route.matches(&sample_signal("x", Severity::Info)));
        assert!(route.matches(&sample_signal("x", Severity::Warn)));
        assert!(route.matches(&sample_signal("x", Severity::Error)));
    }

    #[test]
    fn route_matches_source_pattern() {
        let route = Route {
            name: "cache-monitor".to_string(),
            min_severity: Severity::Trace,
            source_pattern: Some("cache".to_string()),
        };

        assert!(route.matches(&sample_signal("cache-warmer", Severity::Info)));
        assert!(!route.matches(&sample_signal("auth-gateway", Severity::Info)));
    }

    #[test]
    fn router_distributes_to_multiple_channels() {
        let mut router = Router::new();
        router.add_route(Route {
            name: "all-events".to_string(),
            min_severity: Severity::Trace,
            source_pattern: None,
        });
        router.add_route(Route {
            name: "alerts".to_string(),
            min_severity: Severity::Error,
            source_pattern: None,
        });

        let info = sample_signal("x", Severity::Info);
        let error = sample_signal("x", Severity::Error);

        assert_eq!(router.route(&info), vec!["all-events"]);
        assert_eq!(router.route(&error), vec!["all-events", "alerts"]);
    }

    #[test]
    fn batch_routing_counts_correctly() {
        let mut router = Router::new();
        router.add_route(Route {
            name: "sink".to_string(),
            min_severity: Severity::Trace,
            source_pattern: None,
        });

        let signals = generate_test_signals(100);
        let counts = router.route_batch(&signals);

        assert_eq!(*counts.get("sink").unwrap(), 100);
    }

    #[test]
    fn empty_router_routes_nothing() {
        let router = Router::new();
        let signal = sample_signal("x", Severity::Fatal);
        assert!(router.route(&signal).is_empty());
    }

    #[test]
    fn generate_test_signals_is_deterministic() {
        let a = generate_test_signals(50);
        let b = generate_test_signals(50);
        assert_eq!(a.len(), b.len());
        for (sa, sb) in a.iter().zip(b.iter()) {
            assert_eq!(sa.source, sb.source);
            assert_eq!(sa.severity, sb.severity);
            assert_eq!(sa.payload, sb.payload);
        }
    }

    #[test]
    fn route_count_tracks_registrations() {
        let mut router = Router::new();
        assert_eq!(router.route_count(), 0);
        router.add_route(Route {
            name: "a".to_string(),
            min_severity: Severity::Trace,
            source_pattern: None,
        });
        router.add_route(Route {
            name: "b".to_string(),
            min_severity: Severity::Error,
            source_pattern: None,
        });
        assert_eq!(router.route_count(), 2);
    }
}

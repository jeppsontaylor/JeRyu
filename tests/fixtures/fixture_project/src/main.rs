use signal_router::{Route, Router, Severity, generate_test_signals};

fn main() {
    println!("signal-router v0.4.2 — telemetry pipeline routing engine\n");

    let mut router = Router::new();
    router.add_route(Route {
        name: "all-events".to_string(),
        min_severity: Severity::Trace,
        source_pattern: None,
    });
    router.add_route(Route {
        name: "alerts".to_string(),
        min_severity: Severity::Warn,
        source_pattern: None,
    });
    router.add_route(Route {
        name: "cache-monitor".to_string(),
        min_severity: Severity::Trace,
        source_pattern: Some("cache".to_string()),
    });
    router.add_route(Route {
        name: "security-audit".to_string(),
        min_severity: Severity::Error,
        source_pattern: Some("auth".to_string()),
    });

    println!("Registered {} routes", router.route_count());

    let signals = generate_test_signals(1_000);
    let counts = router.route_batch(&signals);

    println!("\nRouting results for 1,000 synthetic signals:");
    let mut entries: Vec<_> = counts.iter().collect();
    entries.sort_by(|a, b| b.1.cmp(a.1));
    for (channel, count) in entries {
        println!("  {channel:20} → {count:>5} signals");
    }
}

use super::*;

pub fn markdown_report(report: &ScanReport) -> String {
    let mut output = String::new();
    output.push_str("# cargo-aer Findings\n\n");
    output.push_str(&format!("Generated: {}\n\n", report.generated_at));
    output.push_str(&format!(
        "Repair hint: {} - {}. {}\n\n",
        report.repair_hint.purpose, report.repair_hint.reason, report.repair_hint.docs_url
    ));
    output.push_str("| Class | Severity | Confidence | Path | Summary | Suggested Fix |\n");
    output.push_str("| --- | --- | --- | --- | --- | --- |\n");
    for finding in &report.findings {
        output.push_str(&format!(
            "| {} | {} | {:.2} | `{}` | {} | {} |\n",
            finding.class_id,
            finding.severity,
            finding.confidence,
            finding.path,
            finding.summary.replace('|', "\\|"),
            finding.suggested_fix.replace('|', "\\|")
        ));
    }
    output
}

pub fn sarif_report(report: &ScanReport) -> serde_json::Value {
    let mut rules = std::collections::BTreeMap::new();
    for finding in &report.findings {
        rules.entry(finding.class_id.clone()).or_insert_with(|| {
            serde_json::json!({
                "id": finding.class_id,
                "name": finding.class_id,
                "shortDescription": { "text": finding.summary },
                "help": { "text": finding.suggested_fix },
            })
        });
    }

    serde_json::json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "cargo-aer",
                    "rules": rules.into_values().collect::<Vec<_>>(),
                }
            },
            "results": report.findings.iter().map(|finding| {
                serde_json::json!({
                    "ruleId": finding.class_id,
                    "level": sarif_level(&finding.severity),
                    "message": { "text": finding.summary },
                    "locations": [{
                        "physicalLocation": {
                            "artifactLocation": { "uri": finding.path }
                        }
                    }]
                })
            }).collect::<Vec<_>>()
        }]
    })
}

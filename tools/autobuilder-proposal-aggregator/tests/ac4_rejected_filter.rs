//! AC4: A #REJECTED: entry in applied.log suppresses its proposal.
use std::fs;
use std::io::Write;
use tempfile::TempDir;

fn write_file(dir: &TempDir, name: &str, content: &str) {
    let path = dir.path().join(name);
    let mut f = fs::File::create(path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

#[test]
fn test_ac4_rejected_filter() {
    let dir = TempDir::new().unwrap();

    // A proposal whose slug is rejected
    write_file(
        &dir,
        "rejected-slug.json",
        r#"{
            "slug": "rejected-slug",
            "suggestions": [
                {
                    "id": "idea-001",
                    "type": "script_patch",
                    "target": "scripts/run.sh",
                    "rationale": "This was a bad idea and got rejected."
                }
            ]
        }"#,
    );

    // Another proposal that's still open
    write_file(
        &dir,
        "open-slug.json",
        r#"{
            "slug": "open-slug",
            "suggestions": [
                {
                    "id": "idea-002",
                    "type": "template_addition",
                    "target": "templates/scaffold/tests/smoke.rs.tmpl",
                    "rationale": "Add a basic smoke test template."
                }
            ]
        }"#,
    );

    // applied.log with #REJECTED: entry for the id
    write_file(
        &dir,
        "applied.log",
        "#REJECTED: idea-001\n",
    );

    let proposals_dir = dir.path().to_str().unwrap();
    let applied_log = dir.path().join("applied.log");
    let result = autobuilder_proposal_aggregator::run(
        proposals_dir,
        applied_log.to_str().unwrap(),
        1,
        "json",
    )
    .unwrap();

    let output: serde_json::Value = serde_json::from_str(&result).unwrap();

    // applied_filtered should be 1 (the rejected one)
    assert_eq!(
        output["coverage"]["applied_filtered"].as_u64().unwrap(),
        1,
        "Expected applied_filtered=1 for rejected idea"
    );

    // Only the open-slug cluster should remain
    let clusters = output["clusters"].as_array().unwrap();
    assert_eq!(clusters.len(), 1, "Expected exactly 1 open cluster");
    assert_eq!(
        clusters[0]["target_file"].as_str().unwrap(),
        "templates/scaffold/tests/smoke.rs.tmpl"
    );
}

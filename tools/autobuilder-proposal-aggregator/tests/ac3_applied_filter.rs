//! AC3: A proposal whose id/sha appears in applied.log is filtered out
//! and counted in coverage.applied_filtered.
use std::fs;
use std::io::Write;
use tempfile::TempDir;

fn write_file(dir: &TempDir, name: &str, content: &str) {
    let path = dir.path().join(name);
    let mut f = fs::File::create(path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

#[test]
fn test_ac3_applied_filter() {
    let dir = TempDir::new().unwrap();

    // A proposal with a known suggestion id
    write_file(
        &dir,
        "slug-x.json",
        r#"{
            "slug": "slug-x",
            "suggestions": [
                {
                    "id": "deadbeef1234",
                    "type": "template_addition",
                    "target": "templates/scaffold/src/lib.rs.tmpl",
                    "rationale": "This suggestion has already been applied."
                },
                {
                    "id": "fresh-idea-99",
                    "type": "template_addition",
                    "target": "templates/scaffold/src/main.rs.tmpl",
                    "rationale": "This suggestion is still open."
                }
            ]
        }"#,
    );

    // applied.log marks deadbeef1234 as applied
    write_file(
        &dir,
        "applied.log",
        "applied-suggestion:deadbeef1234\n",
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

    // applied_filtered should be 1
    assert_eq!(
        output["coverage"]["applied_filtered"].as_u64().unwrap(),
        1,
        "Expected applied_filtered=1"
    );

    // The remaining cluster should target main.rs.tmpl (the unapplied one)
    let clusters = output["clusters"].as_array().unwrap();
    assert_eq!(clusters.len(), 1, "Expected exactly 1 cluster remaining");
    assert_eq!(
        clusters[0]["target_file"].as_str().unwrap(),
        "templates/scaffold/src/main.rs.tmpl"
    );
}

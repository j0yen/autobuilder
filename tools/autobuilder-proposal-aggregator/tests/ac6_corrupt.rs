//! AC6: An unparseable .json is skipped, counted in coverage.unparseable_skipped,
//! and does not abort the run.
use std::fs;
use std::io::Write;
use tempfile::TempDir;

fn write_file(dir: &TempDir, name: &str, content: &str) {
    let path = dir.path().join(name);
    let mut f = fs::File::create(path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

#[test]
fn test_ac6_corrupt() {
    let dir = TempDir::new().unwrap();

    // Valid file
    write_file(
        &dir,
        "valid.json",
        r#"{
            "slug": "valid-slug",
            "suggestions": [
                {
                    "id": "v1",
                    "type": "template_addition",
                    "target": "templates/scaffold/src/lib.rs.tmpl",
                    "rationale": "Add error handling boilerplate to the lib template."
                }
            ]
        }"#,
    );

    // Corrupt JSON file (truncated)
    write_file(&dir, "corrupt.json", r#"{ "slug": "bad", "suggestions": ["#);

    // Another valid file
    write_file(
        &dir,
        "also-valid.json",
        r#"{
            "slug": "also-valid",
            "target_file": "templates/scaffold/src/lib.rs.tmpl",
            "kind": "template_addition",
            "rationale": "Add error handling boilerplate to the lib template."
        }"#,
    );

    let proposals_dir = dir.path().to_str().unwrap();
    let applied_log = dir.path().join("applied.log");

    // Should not panic or return Err
    let result = autobuilder_proposal_aggregator::run(
        proposals_dir,
        applied_log.to_str().unwrap(),
        1,
        "json",
    );
    assert!(result.is_ok(), "Run should not abort on corrupt JSON");

    let output: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();

    // 3 files total (including the corrupt one)
    assert_eq!(
        output["generated_proposals_read"].as_u64().unwrap(),
        3,
        "Expected 3 files read"
    );

    // 1 unparseable skipped
    assert_eq!(
        output["coverage"]["unparseable_skipped"].as_u64().unwrap(),
        1,
        "Expected 1 unparseable_skipped"
    );

    // The two valid files should have been processed → 1 cluster (same target/rationale)
    let clusters = output["clusters"].as_array().unwrap();
    assert!(!clusters.is_empty(), "Expected at least 1 cluster from valid files");
}

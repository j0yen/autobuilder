//! AC2: Files of differing schema shapes (suggestions[] file and top-level PatchSuggestion)
//! are both normalized and clustered.
use std::fs;
use std::io::Write;
use tempfile::TempDir;

fn write_file(dir: &TempDir, name: &str, content: &str) {
    let path = dir.path().join(name);
    let mut f = fs::File::create(path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

#[test]
fn test_ac2_schema_variation() {
    let dir = TempDir::new().unwrap();

    // Shape 1: suggestions[] array
    write_file(
        &dir,
        "slug-a.json",
        r#"{
            "slug": "slug-a",
            "suggestions": [
                {
                    "id": "sug-a1",
                    "type": "script_patch",
                    "target": "scripts/build.sh",
                    "rationale": "PATH must include cargo bin directory for subprocess invocations to resolve."
                }
            ]
        }"#,
    );

    // Shape 2: top-level PatchSuggestion (target_file at root)
    write_file(
        &dir,
        "slug-b.json",
        r#"{
            "slug": "slug-b",
            "target_file": "scripts/build.sh",
            "kind": "script_patch",
            "diff": "@@ -1 +1 @@ export PATH=$HOME/.cargo/bin:$PATH",
            "rationale": "PATH must include cargo bin directory for subprocess invocations to resolve."
        }"#,
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
    let clusters = output["clusters"].as_array().unwrap();

    // Both records should be parsed and clustered (same target_file + similar rationale → 1 cluster)
    assert!(!clusters.is_empty(), "Expected at least one cluster");

    // Coverage should show 2 proposals read
    assert_eq!(
        output["generated_proposals_read"].as_u64().unwrap(),
        2,
        "Expected 2 proposals read"
    );

    // No unparseable files
    assert_eq!(
        output["coverage"]["unparseable_skipped"].as_u64().unwrap(),
        0,
        "Expected 0 unparseable skipped"
    );

    // The cluster targeting scripts/build.sh should exist with recurrence 2
    let build_cluster = clusters
        .iter()
        .find(|c| c["target_file"].as_str() == Some("scripts/build.sh"));
    assert!(
        build_cluster.is_some(),
        "Expected cluster for scripts/build.sh"
    );
    let bc = build_cluster.unwrap();
    assert_eq!(
        bc["recurrence"].as_u64().unwrap(),
        2,
        "Expected recurrence=2 for scripts/build.sh cluster"
    );
}

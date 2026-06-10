//! AC5: --min-recurrence 2 omits single-slug clusters from clusters[]
//! but they remain counted in coverage.clusters_total.
use std::fs;
use std::io::Write;
use tempfile::TempDir;

fn write_file(dir: &TempDir, name: &str, content: &str) {
    let path = dir.path().join(name);
    let mut f = fs::File::create(path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

#[test]
fn test_ac5_min_recurrence() {
    let dir = TempDir::new().unwrap();

    // Two files targeting the SAME target → recurrence=2 cluster
    write_file(
        &dir,
        "slug-p.json",
        r#"{
            "slug": "slug-p",
            "suggestions": [
                {
                    "id": "p1",
                    "type": "template_addition",
                    "target": "templates/scaffold/tests/integration_cli.rs.tmpl",
                    "rationale": "Subprocess binary dispatch needs integration coverage in template."
                }
            ]
        }"#,
    );
    write_file(
        &dir,
        "slug-q.json",
        r#"{
            "slug": "slug-q",
            "suggestions": [
                {
                    "id": "q1",
                    "type": "template_addition",
                    "target": "templates/scaffold/tests/integration_cli.rs.tmpl",
                    "rationale": "Subprocess binary dispatch needs integration coverage in template."
                }
            ]
        }"#,
    );

    // One file targeting a DIFFERENT target → recurrence=1 cluster
    write_file(
        &dir,
        "slug-r.json",
        r#"{
            "slug": "slug-r",
            "suggestions": [
                {
                    "id": "r1",
                    "type": "script_patch",
                    "target": "scripts/unique-target.sh",
                    "rationale": "Very unique suggestion seen only once across all proposals."
                }
            ]
        }"#,
    );

    let proposals_dir = dir.path().to_str().unwrap();
    let applied_log = dir.path().join("applied.log");

    // With min_recurrence=1: all 2 clusters should appear
    let result_1 = autobuilder_proposal_aggregator::run(
        proposals_dir,
        applied_log.to_str().unwrap(),
        1,
        "json",
    )
    .unwrap();
    let output_1: serde_json::Value = serde_json::from_str(&result_1).unwrap();
    assert_eq!(
        output_1["clusters"].as_array().unwrap().len(),
        2,
        "With min_recurrence=1, expected 2 clusters"
    );
    assert_eq!(
        output_1["coverage"]["clusters_total"].as_u64().unwrap(),
        2,
        "clusters_total should be 2"
    );

    // With min_recurrence=2: only the recurrence=2 cluster should appear in clusters[]
    // but clusters_total should still be 2
    let result_2 = autobuilder_proposal_aggregator::run(
        proposals_dir,
        applied_log.to_str().unwrap(),
        2,
        "json",
    )
    .unwrap();
    let output_2: serde_json::Value = serde_json::from_str(&result_2).unwrap();
    let clusters_2 = output_2["clusters"].as_array().unwrap();
    assert_eq!(
        clusters_2.len(),
        1,
        "With min_recurrence=2, expected 1 cluster in output"
    );
    assert_eq!(
        clusters_2[0]["recurrence"].as_u64().unwrap(),
        2,
        "The remaining cluster should have recurrence=2"
    );
    assert_eq!(
        output_2["coverage"]["clusters_total"].as_u64().unwrap(),
        2,
        "clusters_total should still be 2 even when min_recurrence filters output"
    );
}

//! AC7: Output is deterministic — clusters sorted by (recurrence desc, target_file asc),
//! slugs within a cluster sorted.
use std::fs;
use std::io::Write;
use tempfile::TempDir;

fn write_file(dir: &TempDir, name: &str, content: &str) {
    let path = dir.path().join(name);
    let mut f = fs::File::create(path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

#[test]
fn test_ac7_deterministic() {
    let dir = TempDir::new().unwrap();

    // Two proposals both targeting alpha.rs.tmpl → recurrence=2
    write_file(
        &dir,
        "zzz-slug.json",
        r#"{
            "slug": "zzz-slug",
            "suggestions": [
                {
                    "id": "z1",
                    "type": "template_addition",
                    "target": "templates/alpha.rs.tmpl",
                    "rationale": "Alpha template needs integration test coverage for binary dispatch."
                }
            ]
        }"#,
    );
    write_file(
        &dir,
        "aaa-slug.json",
        r#"{
            "slug": "aaa-slug",
            "suggestions": [
                {
                    "id": "a1",
                    "type": "template_addition",
                    "target": "templates/alpha.rs.tmpl",
                    "rationale": "Alpha template needs integration test coverage for binary dispatch."
                }
            ]
        }"#,
    );

    // One proposal targeting beta.rs.tmpl → recurrence=1
    write_file(
        &dir,
        "mmm-slug.json",
        r#"{
            "slug": "mmm-slug",
            "suggestions": [
                {
                    "id": "m1",
                    "type": "template_addition",
                    "target": "templates/beta.rs.tmpl",
                    "rationale": "Beta template needs a unique kind of fix."
                }
            ]
        }"#,
    );

    // One proposal targeting gamma.rs.tmpl → recurrence=1
    write_file(
        &dir,
        "bbb-slug.json",
        r#"{
            "slug": "bbb-slug",
            "suggestions": [
                {
                    "id": "b1",
                    "type": "template_addition",
                    "target": "templates/gamma.rs.tmpl",
                    "rationale": "Gamma template also needs a unique kind of fix."
                }
            ]
        }"#,
    );

    let proposals_dir = dir.path().to_str().unwrap();
    let applied_log = dir.path().join("applied.log");

    // Run twice and compare
    let result_1 = autobuilder_proposal_aggregator::run(
        proposals_dir,
        applied_log.to_str().unwrap(),
        1,
        "json",
    )
    .unwrap();
    let result_2 = autobuilder_proposal_aggregator::run(
        proposals_dir,
        applied_log.to_str().unwrap(),
        1,
        "json",
    )
    .unwrap();

    assert_eq!(result_1, result_2, "Output must be deterministic across runs");

    let output: serde_json::Value = serde_json::from_str(&result_1).unwrap();
    let clusters = output["clusters"].as_array().unwrap();

    // 3 clusters total (alpha=2, beta=1, gamma=1)
    assert_eq!(clusters.len(), 3, "Expected 3 clusters");

    // First cluster: alpha (recurrence=2, highest)
    assert_eq!(
        clusters[0]["target_file"].as_str().unwrap(),
        "templates/alpha.rs.tmpl",
        "First cluster should be alpha (highest recurrence)"
    );
    assert_eq!(
        clusters[0]["recurrence"].as_u64().unwrap(),
        2,
        "First cluster recurrence should be 2"
    );

    // Slugs within alpha cluster should be sorted: aaa-slug before zzz-slug
    let alpha_slugs: Vec<&str> = clusters[0]["slugs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(alpha_slugs, vec!["aaa-slug", "zzz-slug"], "Slugs must be sorted");

    // Second cluster: beta (recurrence=1, comes before gamma alphabetically)
    assert_eq!(
        clusters[1]["target_file"].as_str().unwrap(),
        "templates/beta.rs.tmpl",
        "Second cluster should be beta (recurrence=1, alphabetically before gamma)"
    );

    // Third cluster: gamma
    assert_eq!(
        clusters[2]["target_file"].as_str().unwrap(),
        "templates/gamma.rs.tmpl",
        "Third cluster should be gamma"
    );
}

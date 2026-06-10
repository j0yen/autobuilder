//! AC1: Two files for distinct slugs both targeting integration_cli.rs.tmpl
//! → one cluster with recurrence:2 and both slugs listed.
use std::fs;
use std::io::Write;
use tempfile::TempDir;

fn write_file(dir: &TempDir, name: &str, content: &str) {
    let path = dir.path().join(name);
    let mut f = fs::File::create(path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

#[test]
fn test_ac1_recurrence() {
    let dir = TempDir::new().unwrap();

    // File 1: suggestions[] shape, slug=mqo-spec
    write_file(
        &dir,
        "mqo-spec.json",
        r#"{
            "slug": "mqo-spec",
            "suggestions": [
                {
                    "id": "sug-001",
                    "type": "template_addition",
                    "target": "templates/scaffold/tests/integration_cli.rs.tmpl",
                    "rationale": "Subprocess-orchestration projects have binary dispatch arms cargo test cannot reach."
                }
            ]
        }"#,
    );

    // File 2: suggestions[] shape, slug=mqo-mcp-server
    write_file(
        &dir,
        "mqo-mcp-server.json",
        r#"{
            "slug": "mqo-mcp-server",
            "suggestions": [
                {
                    "id": "sug-002",
                    "type": "template_addition",
                    "target": "templates/scaffold/tests/integration_cli.rs.tmpl",
                    "rationale": "Subprocess-orchestration projects have binary dispatch arms cargo test cannot reach."
                }
            ]
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

    // Exactly one cluster
    assert_eq!(clusters.len(), 1, "Expected 1 cluster, got {}", clusters.len());

    let cluster = &clusters[0];
    assert_eq!(
        cluster["recurrence"].as_u64().unwrap(),
        2,
        "Expected recurrence=2"
    );

    let slugs: Vec<&str> = cluster["slugs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        slugs.contains(&"mqo-spec"),
        "Expected mqo-spec in slugs: {slugs:?}"
    );
    assert!(
        slugs.contains(&"mqo-mcp-server"),
        "Expected mqo-mcp-server in slugs: {slugs:?}"
    );

    assert_eq!(
        cluster["target_file"].as_str().unwrap(),
        "templates/scaffold/tests/integration_cli.rs.tmpl"
    );
}

use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
fn ac4_implicit_bin_detected() {
    let dir = fixture("implicit_bin");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_autobuilder-bincov-receipt"))
        .arg(dir.to_str().unwrap())
        .arg("--format")
        .arg("json")
        .output()
        .expect("failed to run binary");

    assert!(output.status.success(), "should exit 0 (concern without --strict): {output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|_| panic!("invalid JSON: {stdout}"));

    assert_eq!(v["has_bin"], true, "src/main.rs without explicit [[bin]] should be detected as has_bin:true");
}

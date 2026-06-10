use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
fn ac3_pure_lib_no_bin() {
    let dir = fixture("pure_lib");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_autobuilder-bincov-receipt"))
        .arg(dir.to_str().unwrap())
        .arg("--format")
        .arg("json")
        .output()
        .expect("failed to run binary");

    assert!(output.status.success(), "should exit 0: {output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|_| panic!("invalid JSON: {stdout}"));

    assert_eq!(v["has_bin"], false, "has_bin should be false for pure lib");
    assert_eq!(v["verdict"], "pass", "verdict should be pass");
}

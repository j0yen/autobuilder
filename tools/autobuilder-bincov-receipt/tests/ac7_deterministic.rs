use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn run_receipt(dir: &Path) -> String {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_autobuilder-bincov-receipt"))
        .arg(dir.to_str().unwrap())
        .arg("--format")
        .arg("json")
        .output()
        .expect("failed to run binary");
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn ac7_json_output_deterministic() {
    let dir = fixture("bin_no_test");
    let run1 = run_receipt(&dir);
    let run2 = run_receipt(&dir);
    let run3 = run_receipt(&dir);

    assert_eq!(run1, run2, "output differs between run1 and run2");
    assert_eq!(run2, run3, "output differs between run2 and run3");

    // Also verify schema fields are present
    let v: serde_json::Value = serde_json::from_str(&run1).expect("valid JSON");
    assert!(v["receipt"].is_string());
    assert!(v["crate"].is_string());
    assert!(v["has_bin"].is_boolean());
    assert!(v["bin_names"].is_array());
    assert!(v["has_integration_test"].is_boolean());
    assert!(v["integration_test_files"].is_array());
    assert!(v["verdict"].is_string());
    assert!(v["note"].is_string());
}

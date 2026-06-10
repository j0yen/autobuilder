use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
fn ac5_strict_exits_3_on_concern() {
    let dir = fixture("bin_no_test");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_autobuilder-bincov-receipt"))
        .arg(dir.to_str().unwrap())
        .arg("--strict")
        .output()
        .expect("failed to run binary");

    let code = output.status.code().expect("no exit code");
    assert_eq!(code, 3, "strict mode should exit 3 on concern");
}

#[test]
fn ac5_strict_exits_0_on_pass() {
    let dir = fixture("bin_with_test");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_autobuilder-bincov-receipt"))
        .arg(dir.to_str().unwrap())
        .arg("--strict")
        .output()
        .expect("failed to run binary");

    assert!(output.status.success(), "strict mode should exit 0 on pass");
}

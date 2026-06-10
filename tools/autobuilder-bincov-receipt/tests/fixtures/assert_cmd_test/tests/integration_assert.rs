use assert_cmd::Command;

#[test]
fn test_assert_cmd_crate_runs() {
    let mut cmd = Command::cargo_bin("assert-cmd-crate").unwrap();
    cmd.assert().success();
}

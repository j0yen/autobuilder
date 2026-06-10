use std::process::Command;

#[test]
fn test_foo_runs() {
    let output = Command::new(env!("CARGO_BIN_EXE_foo"))
        .output()
        .expect("failed to run foo");
    assert!(output.status.success());
}

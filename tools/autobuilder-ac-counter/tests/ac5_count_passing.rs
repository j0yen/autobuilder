use autobuilder_ac_counter::count_passing;

const SAMPLE_OUTPUT: &str = r#"
running 6 tests
test ac1_a ... ok
test new_ac1_b ... ok
test ext1_c ... ok
test acceptance_ac1 ... ok
test ac2_fails ... FAILED
test helper_skipped ... ok
test result: FAILED. 5 passed; 1 failed;
"#;

#[test]
fn ac5_count_passing_correct() {
    // ac1_a, new_ac1_b, ext1_c, acceptance_ac1 = 4 passing ACs
    // ac2_fails = FAILED (not counted)
    // helper_skipped = not an AC pattern (not counted)
    let count = count_passing(SAMPLE_OUTPUT);
    assert_eq!(count, 4, "expected 4 passing ACs, got {count}");
}

#[test]
fn ac5_failed_not_counted() {
    let stdout = "test ac1_broken ... FAILED\ntest ac2_good ... ok\n";
    assert_eq!(count_passing(stdout), 1);
}

#[test]
fn ac5_empty_stdout() {
    assert_eq!(count_passing(""), 0);
    assert_eq!(count_passing("no tests here"), 0);
}

use autobuilder_ac_counter::discover;
use std::path::Path;

#[test]
fn ac6_missing_tests_dir_no_panic() {
    // Empty fixture dir has no tests/ subdirectory at all
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/empty");
    let inv = discover(&fixture).expect("discover should not error on missing tests/");
    assert_eq!(inv.total, 0, "expected total=0 for missing tests/, got {}", inv.total);
    assert_eq!(inv.by_layout.split_file, 0);
    assert_eq!(inv.by_layout.monolithic_fns, 0);
    assert_eq!(inv.by_layout.mock_files, 0);
    assert!(inv.names.is_empty());
}

#[test]
fn ac6_nonexistent_crate_dir() {
    // A path that doesn't exist at all should also return Ok with total=0
    let nonexistent = Path::new("/tmp/this-path-does-not-exist-ac6-test-fixture");
    let inv = discover(nonexistent).expect("discover should not error on nonexistent dir");
    assert_eq!(inv.total, 0);
}

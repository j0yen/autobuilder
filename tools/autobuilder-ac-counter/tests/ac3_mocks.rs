use autobuilder_ac_counter::discover;
use std::path::Path;

#[test]
fn ac3_mock_files_counted() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mocks_only");
    let inv = discover(&fixture).expect("discover should succeed");
    assert_eq!(inv.by_layout.mock_files, 2, "expected mock_files=2, got {}", inv.by_layout.mock_files);
    assert_eq!(inv.total, 2, "expected total=2");
    assert_eq!(inv.by_layout.split_file, 0);
    assert_eq!(inv.by_layout.monolithic_fns, 0);
}

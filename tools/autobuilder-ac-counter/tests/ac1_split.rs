use autobuilder_ac_counter::discover;
use std::path::Path;

#[test]
fn ac1_split_file_three_files() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/split");
    let inv = discover(&fixture).expect("discover should succeed");
    assert_eq!(inv.total, 3, "expected total=3 for split layout");
    assert_eq!(inv.by_layout.split_file, 3, "expected split_file=3");
    assert_eq!(inv.by_layout.monolithic_fns, 0);
    assert_eq!(inv.by_layout.mock_files, 0);
}

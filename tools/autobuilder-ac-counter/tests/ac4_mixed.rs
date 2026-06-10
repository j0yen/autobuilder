use autobuilder_ac_counter::discover;
use std::path::Path;

/// Mixed layout: 2 split-file + 2 monolithic_fns + 1 mock = 5 total
#[test]
fn ac4_mixed_layout_correct_total() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mixed");
    let inv = discover(&fixture).expect("discover should succeed");
    assert_eq!(inv.by_layout.split_file, 2, "expected split_file=2, got {}", inv.by_layout.split_file);
    assert_eq!(inv.by_layout.monolithic_fns, 2, "expected monolithic_fns=2, got {}", inv.by_layout.monolithic_fns);
    assert_eq!(inv.by_layout.mock_files, 1, "expected mock_files=1, got {}", inv.by_layout.mock_files);
    assert_eq!(inv.total, 5, "expected total=5, got {}", inv.total);
}

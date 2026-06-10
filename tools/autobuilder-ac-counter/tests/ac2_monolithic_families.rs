use autobuilder_ac_counter::discover;
use std::path::Path;

/// THE KEY FIX: all three families (ac, new_ac, ext) must be counted.
#[test]
fn ac2_monolithic_all_three_families() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/monolithic");
    let inv = discover(&fixture).expect("discover should succeed");
    assert_eq!(inv.total, 3, "expected total=3, got {}", inv.total);
    assert_eq!(inv.by_layout.monolithic_fns, 3, "all three fn families must be counted");
    assert_eq!(inv.by_layout.split_file, 0);
    assert_eq!(inv.by_layout.mock_files, 0);

    let names = &inv.names;
    assert!(names.contains(&"ac1_x".to_string()), "names must include ac1_x, got {:?}", names);
    assert!(names.contains(&"new_ac1_y".to_string()), "names must include new_ac1_y, got {:?}", names);
    assert!(names.contains(&"ext1_z".to_string()), "names must include ext1_z, got {:?}", names);
}

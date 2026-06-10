use autobuilder_ac_counter::discover;
use std::path::Path;

#[test]
fn ac7_names_sorted_and_deterministic() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mixed");

    // Run discover twice — results must be identical
    let inv1 = discover(&fixture).expect("first discover");
    let inv2 = discover(&fixture).expect("second discover");

    assert_eq!(inv1, inv2, "discover must be deterministic");

    // names must be strictly sorted
    let names = &inv1.names;
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, &sorted, "names must be sorted: {:?}", names);

    // Verify no duplicates
    let unique: std::collections::HashSet<_> = names.iter().collect();
    assert_eq!(unique.len(), names.len(), "names must not contain duplicates");
}

#[test]
fn ac7_split_names_sorted() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/split");
    let inv = discover(&fixture).expect("discover split fixture");

    // alpha, beta, gamma — sorted lexicographically
    let names = &inv.names;
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, &sorted, "split-file names must be sorted: {:?}", names);
}

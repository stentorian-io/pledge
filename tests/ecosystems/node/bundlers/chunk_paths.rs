//! Chunk paths compatibility checks.

use interlint::ecosystems::node::discovery::entry_points::resolve_entry_points;
use std::path::Path;

#[test]
fn hashed_filename_normalized_in_pipeline() {
    // CHUNK-01 normalization is exercised via the fixture-based test fixture_vite_chunk_normalization_no_spurious_path_findings.
    // Here we verify the entry point resolution on a vite-like package fixture.
    let fixture_old =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/node/bundlers/vite/old");
    assert!(fixture_old.is_dir(), "missing Vite fixture");
    // Resolve entry points: confirm index.js is found
    let eps = resolve_entry_points(&fixture_old).unwrap();
    let names: Vec<String> = eps
        .iter()
        .map(|e| e.path.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert!(
        names.iter().any(|n| n == "index.js"),
        "CHUNK-01: index.js entry point not found. Got: {:?}",
        names
    );
}

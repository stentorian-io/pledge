//! Locate committed inputs without depending on the test process directory.

use std::path::PathBuf;

pub(crate) fn fixture_path(relative: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(relative);
    assert!(path.exists(), "missing test fixture: {}", path.display());
    path
}

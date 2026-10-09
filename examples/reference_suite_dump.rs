//! Print a candidate reference snapshot for manual review.

#[path = "../tests/ecosystems/node/reference/support.rs"]
mod reference;

fn main() {
    let package = std::env::args()
        .nth(1)
        .expect("usage: reference_suite_dump <package>");
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/node/reference")
        .join(package);
    let unpack = |name| {
        interlint::acquire::extract_tgz_from_bytes(&std::fs::read(directory.join(name)).unwrap())
            .unwrap()
    };
    let (_old_dir, old_root) = unpack("from.tgz");
    let (_new_dir, new_root) = unpack("to.tgz");
    let snapshot = reference::run_pipeline(&old_root, &new_root);
    println!("{}", serde_json::to_string_pretty(&snapshot).unwrap());
}

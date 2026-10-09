//! Pinned package comparisons. Structural analysis is explicitly incomplete;
//! real language-server behavior is exercised by the opt-in LSP suites.

mod support;

fn run_reference_case(package: &str) {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/node/reference")
        .join(package);
    let unpack = |name| {
        interlint::ecosystems::node::acquire::extract_tgz_from_bytes(
            &std::fs::read(directory.join(name)).unwrap(),
        )
        .unwrap()
    };
    let (_old_dir, old_root) = unpack("from.tgz");
    let (_new_dir, new_root) = unpack("to.tgz");
    let expected: support::Snapshot =
        serde_json::from_slice(&std::fs::read(directory.join("expected-findings.json")).unwrap())
            .unwrap();
    let actual = support::run_pipeline(&old_root, &new_root);
    assert_eq!(actual, expected, "reference mismatch for {package}");
}

#[test]
fn reference_react() {
    run_reference_case("react");
}
#[test]
fn reference_tanstack_query() {
    run_reference_case("tanstack-query");
}
#[test]
fn reference_trpc() {
    run_reference_case("trpc");
}
#[test]
fn reference_zod() {
    run_reference_case("zod");
}
#[test]
fn reference_prisma() {
    run_reference_case("prisma");
}

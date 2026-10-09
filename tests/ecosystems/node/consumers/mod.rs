#![cfg(feature = "lsp-tests")]

mod support;

#[test]
fn package_resolution_agrees_with_node_consumers() {
    support::assert_consumer_contracts(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/node/packages/consumer-resolution.json"
    )));
}

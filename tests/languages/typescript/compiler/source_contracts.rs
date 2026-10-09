//! Compare findings with independent TypeScript compilation.
#![cfg(feature = "lsp-tests")]

#[test]
fn findings_agree_with_consumers() {
    crate::node_consumers::assert_consumer_contracts(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/typescript/compiler/source-contracts.json"
    )));
}

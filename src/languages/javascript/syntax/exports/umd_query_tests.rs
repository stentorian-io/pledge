use crate::languages::javascript::syntax::queries::UMD_FACTORY_WRAPPER_QUERY;
use tree_sitter::{Language, Query};

#[test]
fn umd_factory_wrapper_query_compiles_for_javascript() {
    // A future tree-sitter-javascript
    // grammar version bump may rename `function_expression` or
    // `formal_parameters`. This unit test fails LOUDLY when that happens
    // (instead of the production `if let Ok(query) =...` silently
    // skipping detection). Do NOT swap `.expect(...)` for `.ok()`.
    let lang: Language = tree_sitter_javascript::LANGUAGE.into();
    Query::new(&lang, UMD_FACTORY_WRAPPER_QUERY)
        .expect("UMD_FACTORY_WRAPPER_QUERY must compile against tree-sitter-javascript 0.25.0");
}

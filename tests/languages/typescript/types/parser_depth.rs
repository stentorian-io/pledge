//! Parser depth compatibility checks.

use pledge::languages::typescript::types::parser::parse_type_string;

#[test]
fn deeply_nested_type_no_stack_overflow() {
    let mut input = String::from("string");
    for _ in 0..100 {
        input = format!("Array<{}>", input);
    }
    // Must not panic or stack overflow — depth guard in parser.rs limits recursion
    let _shape = parse_type_string(&input);
}

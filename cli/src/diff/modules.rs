use crate::ecma::parser::parse_esm_entry_into_ast;
use crate::ecma::walker::get_exports_in_module;
use crate::pkg::entries::PkgEntry;

pub fn diff_modules(previous: &PkgEntry, current: &PkgEntry) -> anyhow::Result<u32> {
    let mut red_flag_count: u32 = 0;

    let previous_module = parse_esm_entry_into_ast(previous)?;
    let current_module = parse_esm_entry_into_ast(current)?;

    let (previous_default_export, previous_named_exports) = get_exports_in_module(&previous_module);
    let (current_default_export, current_named_exports) = get_exports_in_module(&current_module);

    if previous_default_export.is_some() && current_default_export.is_none() {
        red_flag_count += 1;

        println!(
            "BREAKING CHANGE: Default export in '{}' was removed.",
            previous.name
        );
    }

    for (previous_export_name, _) in previous_named_exports.iter() {
        if !current_named_exports.contains_key(previous_export_name) {
            red_flag_count += 1;

            println!(
                "BREAKING CHANGE: Named export '{}' in '{}' was removed or renamed.",
                previous_export_name, previous.name
            );
        }
    }

    Ok(red_flag_count)
}

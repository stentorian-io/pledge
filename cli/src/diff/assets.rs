use crate::ecma::entity::EntityDeclaration;
use crate::ecma::parser::parse_pkg_entry;
use crate::ecma::walker::get_exports_in_module;
use crate::pkg::contents::PkgContents;
use crate::pkg::entries::PkgEntry;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;

pub fn diff_pkg_assets(
    previous_contents: &PkgContents,
    current_contents: &PkgContents,
) -> Result<Vec<PathBuf>> {
    let previous_assets = previous_contents
        .asset_list()
        .with_context(|| "Failed to get list of previous package's assets.")?;

    let current_assets = current_contents
        .asset_list()
        .with_context(|| "Failed to get list of current package's assets.")?;

    Ok(previous_assets
        .difference(&current_assets)
        .map(|asset_path| asset_path.to_owned())
        .collect())
}

pub fn diff_pkg_entries<'entry>(
    previous_entries: &'entry HashMap<String, PkgEntry>,
    current_entries: &'entry HashMap<String, PkgEntry>,
) -> Result<(
    Vec<&'entry String>,
    HashMap<&'entry String, (&'entry PkgEntry, &'entry PkgEntry)>,
)> {
    let mut missing_entries = Vec::new();
    let mut matching_entries = HashMap::new();

    for (previous_entry_name, previous_entry) in previous_entries.iter() {
        let matching_current_entry = current_entries.get(previous_entry_name);

        if matching_current_entry.is_none() {
            missing_entries.push(previous_entry_name);
        } else {
            matching_entries.insert(
                previous_entry_name,
                (previous_entry, matching_current_entry.unwrap()),
            );
        }
    }

    Ok((missing_entries, matching_entries))
}

pub fn diff_pkg_entry_exports(
    previous_entry: &PkgEntry,
    current_entry: &PkgEntry,
) -> Result<(
    bool,
    Option<EntityDeclaration>,
    Vec<String>,
    HashMap<String, EntityDeclaration>,
)> {
    let mut is_default_export_missing = false;
    let mut matching_default_export = None;

    let mut missing_named_exports = Vec::new();
    let mut matching_named_exports = HashMap::new();

    let previous_module = parse_pkg_entry(previous_entry).with_context(|| {
        format!(
            "Failed to parse previous package entry module: {}",
            previous_entry.name
        )
    })?;

    let current_module = parse_pkg_entry(current_entry).with_context(|| {
        format!(
            "Failed to parse current package entry module: {}",
            current_entry.name
        )
    })?;

    let (previous_default_export, previous_named_exports) =
        get_exports_in_module(previous_entry.dir_path(), previous_module).with_context(|| {
            format!(
                "Failed to get exports from previous package entry module: {}",
                previous_entry.name
            )
        })?;

    let (current_default_export, mut current_named_exports) =
        get_exports_in_module(current_entry.dir_path(), current_module).with_context(|| {
            format!(
                "Failed to get exports from current package entry module: {}",
                current_entry.name
            )
        })?;

    if previous_default_export.is_some() {
        if current_default_export.is_some() {
            matching_default_export = current_default_export;
        } else {
            is_default_export_missing = true;
        }
    }

    for (previous_export_name, _) in previous_named_exports {
        let matching_named_export = current_named_exports.remove(&previous_export_name);

        if let Some(matching_named_export) = matching_named_export {
            matching_named_exports.insert(previous_export_name, matching_named_export);
        } else {
            missing_named_exports.push(previous_export_name);
        }
    }

    Ok((
        is_default_export_missing,
        matching_default_export,
        missing_named_exports,
        matching_named_exports,
    ))
}

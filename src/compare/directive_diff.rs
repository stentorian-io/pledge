//! INF-05 (Phase 19): triple-slash directive diff emission.
//!
//! Stub entry point. The body is filled in by Plan 02; Plan 01 only
//! ships the function signature and wiring so Plan 02 does not need to
//! touch `src/compare/mod.rs`.
//!
//! NOTE: The `DirectiveRecord` type is re-exported from
//! `crate::discover::comment_scan::DirectiveRecord` once that module lands
//! (Task 2 of Plan 01). Task 1 ships this file importing from there; if
//! Task 2 is not yet applied, the build surfaces the missing import — this
//! is intentional: Tasks 1 and 2 form a single atomic foundation delivery
//! per the plan's execute-plan Wave 1 ordering.

use std::path::Path;

use crate::discover::comment_scan::DirectiveRecord;
use crate::types::Finding;

/// Compare two ordered directive lists from the same file pair.
/// Returns findings for added, removed, changed, and reordered directives.
///
/// Plan 01 stub: returns an empty Vec. Plan 02 implements the diff.
#[allow(dead_code, unused_variables)]
pub fn compare_directives(
    old: &[DirectiveRecord],
    new: &[DirectiveRecord],
    file: &Path,
) -> Vec<Finding> {
    Vec::new()
}

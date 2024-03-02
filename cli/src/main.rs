use crate::diff::diff_between;
use anyhow::{bail, Result};
use std::env;
use std::path::PathBuf;

mod ast;
mod diff;
mod npm;
mod path;
mod pkg;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len().ne(&2) {
        bail!("Expected only one argument: the package path.");
    }

    let working_dir = args.last().unwrap();
    let working_dir = PathBuf::from(working_dir);

    let pkg_current = npm::load_from_dir(working_dir)?;
    let pkg_previous = npm::fetch_from_registry(&pkg_current)?;

    diff_between(pkg_previous, pkg_current)?;

    Ok(())
}

use anyhow::{bail, Result};
use std::env;
use std::path::PathBuf;

mod ast;
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

    let previous_files = pkg_previous.contents.resolved_files;
    let current_files = pkg_current.contents.resolved_files;

    let files_missing_in_previous = previous_files.difference(&current_files);
    let has_files_missing_in_previous = files_missing_in_previous.size_hint().1.unwrap() > 0;

    if has_files_missing_in_previous {
        println!("Previous version contains files missing in current:\n");

        for file in files_missing_in_previous {
            println!("{}", file.to_str().unwrap());
        }
    }

    Ok(())
}

use crate::pkg::Pkg;
use std::env;
use std::error::Error;
use std::path::PathBuf;

mod npm;
mod pkg;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len().ne(&2) {
        return Err("Expected only one argument: the package path.".into());
    }

    let pkg_path = PathBuf::from(args.last().unwrap());

    let pkg_current = Pkg::new(pkg_path)?;
    let pkg_previous = npm::fetch_latest_of(&pkg_current)?;

    let files_missing_in_previous = pkg_previous.files.difference(&pkg_current.files);
    let has_files_missing_in_previous = files_missing_in_previous.size_hint().1.unwrap() > 0;

    if has_files_missing_in_previous {
        println!("Previous version contains files missing in current:\n");

        for file in files_missing_in_previous {
            println!("{}", file.to_str().unwrap());
        }
    }

    Ok(())
}

use crate::pkg::Pkg;
use std::env;
use std::error::Error;
use std::path::PathBuf;

mod npm;
mod path;
mod pkg;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len().ne(&2) {
        return Err("Expected only one argument: the package path.".into());
    }

    let pkg_path = PathBuf::from(args.last().unwrap());
    let pkg_json = Pkg::parse_config_in_dir(&pkg_path)?;
    let registry_url = Pkg::get_registry_url(&pkg_path)?;

    println!("Will use {} as registry.", &registry_url);

    let mut pkg_current = Pkg::new(pkg_path, pkg_json, registry_url)?;
    let pkg_previous = npm::fetch_latest_of(&pkg_current)?;

    pkg_current
        .contents
        .resolve_contents_in_dir(&pkg_current.dir_path)?;

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

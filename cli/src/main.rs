use crate::npm::Npm;
use crate::pkg::Pkg;
use std::env;
use std::error::Error;
use std::path::PathBuf;

mod npm;
mod pkg;
mod tar;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len().ne(&2) {
        return Err("Expected only one argument: the package path.".into());
    }

    let pkg_path = PathBuf::from(args.last().unwrap());

    let pkg_current = Pkg::new(&pkg_path)?;
    let pkg_previous = Npm::fetch_latest_pkg_of(&pkg_current)?;

    Npm::download_pkg_if_needed(&pkg_previous, &pkg_path)?;

    Ok(())
}

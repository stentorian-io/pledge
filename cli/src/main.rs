use crate::npm::Npm;
use crate::pkg::Pkg;
use std::env;
use std::error::Error;

mod npm;
mod pkg;
mod tar;

fn get_pkg_path_from_args() -> Result<String, Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len().ne(&2) {
        return Err("Expected only one argument: the package path.".into());
    }

    Ok(args.last().unwrap().to_string())
}

fn main() -> Result<(), Box<dyn Error>> {
    let pkg_current = Pkg::new(&get_pkg_path_from_args()?)?;
    let pkg_previous = Npm::fetch_latest_pkg_of(&pkg_current)?;

    Npm::download_pkg_if_needed(&pkg_previous)?;

    Ok(())
}

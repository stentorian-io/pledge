use crate::npm::Npm;
use crate::pkg::Pkg;
use std::env;
use std::error::Error;

mod npm;
mod pkg;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len().ne(&2) {
        return Err("Expected only one argument: the package path.".into());
    }

    let pkg_path = args.last().unwrap().as_str();
    let pkg = Pkg::new(pkg_path)?;

    let last_version = Npm::fetch_last_version_of(&pkg)?;

    println!("Last version: {}", last_version);

    Ok(())
}

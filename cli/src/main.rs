use crate::pkg::Pkg;
use std::env;
use std::error::Error;
use std::path::PathBuf;

mod pkg;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len().ne(&2) {
        return Err("Expected only one argument: the package path.".into());
    }

    let pkg_path = args.get(1).unwrap();
    let pkg_path = PathBuf::from(pkg_path);

    if !pkg_path.is_dir() {
        return Err("Expected package path to be a directory.".into());
    }

    let pkg_path = pkg_path.join("package.json");

    if !pkg_path.is_file() {
        return Err("Expected package path to contain a package.json file.".into());
    }

    let pkg = Pkg::new(pkg_path.to_str().unwrap())?;

    println!("{:?}", pkg);

    Ok(())
}

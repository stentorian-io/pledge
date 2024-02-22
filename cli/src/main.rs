use crate::pkg::Pkg;
use std::env;
use std::error::Error;

mod pkg;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len().ne(&2) {
        return Err("Expected only one argument: the package path.".into());
    }

    let pkg_path = args.last().unwrap().as_str();
    let pkg = Pkg::new(pkg_path)?;


    println!("Loaded package {}:{}.", pkg.name(), pkg.version());
    println!("Registry URL is {}.", pkg.registry_url());

    Ok(())
}

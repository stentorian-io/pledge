use crate::pkg::Pkg;
use std::env;
use std::error::Error;

mod pkg;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len().ne(&2) {
        return Err("Expected only one argument: the package path.".into());
    }

    let pkg = args.get(1).unwrap();
    let pkg = Pkg::new(pkg.to_str().unwrap())?;


    println!("Loaded package {}:{}.", pkg.name(), pkg.version());

    Ok(())
}

use crate::fs::file::FileExt;
use crate::pkg::entries::PkgEntry;
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use swc_common::errors::{ColorConfig, Handler};
use swc_common::input::StringInput;
use swc_common::sync::Lrc;
use swc_common::{FileName, SourceFile, SourceMap};
use swc_ecma_ast::{EsVersion, Module};
use swc_ecma_parser::lexer::Lexer;
use swc_ecma_parser::{Parser, Syntax};

pub fn parse_pkg_entry(entry: &PkgEntry) -> Result<Module> {
    println!(
        "Loading '{}' module '{}'...",
        entry.name,
        entry.path.display()
    );

    // Entry module files are loaded from a PkgEntry's load_file method instead of the
    // SourceMap equivalent since we load a file either from disk or from a tarball.
    let file_data = match entry.load_file()? {
        Some(data) => String::from_utf8(data)?,
        None => bail!("Entry module '{}' does not exist.", entry.path.display()),
    };

    let source_map: Lrc<SourceMap> = Default::default();
    let source_name = FileName::Real(entry.path.to_owned());
    let source_file = source_map.new_source_file(source_name, file_data);

    parse_source_file(source_map, Rc::clone(&source_file))
}

pub fn parse_import(file_path: &Path) -> Result<Module> {
    let source_map: Lrc<SourceMap> = Default::default();
    let source_file = source_map.load_file(file_path)?;

    parse_source_file(source_map, Rc::clone(&source_file))
}

fn parse_source_file(source_map: Lrc<SourceMap>, source_file: Rc<SourceFile>) -> Result<Module> {
    let source_file_path = PathBuf::from(source_file.name.to_string());
    let source_file_ext = FileExt::from(&source_file_path)?;

    let syntax = if source_file_ext.is_ts() {
        Syntax::Typescript(Default::default())
    } else {
        Syntax::Es(Default::default())
    };

    let lexer = Lexer::new(
        syntax,
        EsVersion::EsNext,
        StringInput::from(&*source_file),
        None,
    );

    let mut parser = Parser::new_from(lexer);
    let handler = Handler::with_tty_emitter(ColorConfig::Auto, true, false, Some(source_map));

    for parse_error in parser.take_errors() {
        parse_error.into_diagnostic(&handler).emit();
    }

    let module = if source_file_ext.is_ts() {
        parser.parse_typescript_module()
    } else {
        parser.parse_module()
    };

    if let Err(parse_error) = module {
        parse_error.into_diagnostic(&handler).emit();

        bail!(
            "Failed to parse entry module '{}'.",
            source_file_path.display()
        );
    }

    Ok(module.unwrap())
}

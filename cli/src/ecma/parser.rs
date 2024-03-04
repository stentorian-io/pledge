use crate::pkg::entries::PkgEntry;
use anyhow::{bail, Result};
use swc_common::errors::{ColorConfig, Handler};
use swc_common::input::StringInput;
use swc_common::sync::Lrc;
use swc_common::{FileName, SourceMap};
use swc_ecma_ast::{EsVersion, Module};
use swc_ecma_parser::lexer::Lexer;
use swc_ecma_parser::{Parser, Syntax};

pub fn parse_esm_entry_into_ast(entry: &PkgEntry) -> Result<Module> {
    println!(
        "Loading '{}' module '{}'...",
        entry.name,
        entry.path.display()
    );

    let file_data = entry.load_file()?;
    let file_data = match file_data {
        Some(data) => String::from_utf8(data)?,
        None => bail!("Entry module '{}' does not exist.", entry.path.display()),
    };

    let source_map: Lrc<SourceMap> = Default::default();
    let source_file = source_map.new_source_file(FileName::Anon, file_data);

    let syntax = if entry.ext.is_ts() {
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

    let module = if entry.ext.is_ts() {
        parser.parse_typescript_module()
    } else {
        parser.parse_module()
    };

    if let Err(parse_error) = module {
        parse_error.into_diagnostic(&handler).emit();
        bail!("Failed to parse entry module '{}'.", entry.name);
    }

    Ok(module.unwrap())
}

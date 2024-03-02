use anyhow::{bail, Result};
use std::path::Path;
use swc_common::errors::{ColorConfig, Handler};
use swc_common::input::StringInput;
use swc_common::sync::Lrc;
use swc_common::SourceMap;
use swc_ecma_ast::{EsVersion, Module};
use swc_ecma_parser::lexer::Lexer;
use swc_ecma_parser::{Parser, Syntax};

pub fn parse_esm_module(file_path: &Path) -> Result<Module> {
    let is_ts_file = match file_path.extension().unwrap().to_str().unwrap() {
        "ts" | "tsx" => true,
        _ => false,
    };

    let source_map: Lrc<SourceMap> = Default::default();
    let source_file = source_map.load_file(file_path)?;

    let syntax = if is_ts_file {
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

    let module = if is_ts_file {
        parser.parse_typescript_module()
    } else {
        parser.parse_module()
    };

    if let Err(parse_error) = module {
        parse_error.into_diagnostic(&handler).emit();
        bail!("Failed to parse module '{}'.", file_path.display());
    }

    Ok(module.unwrap())
}

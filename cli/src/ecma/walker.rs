use crate::ecma::entity::EntityDeclaration;
use crate::ecma::parser::parse_import;
use anyhow::{bail, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use swc_ecma_ast::{
    ExportAll, ExportDecl, ExportDefaultDecl, ExportDefaultExpr, ExportNamedSpecifier,
    ExportSpecifier, Module, ModuleExportName, NamedExport,
};

pub type ExternalSpecifiers<'module> = HashMap<PathBuf, &'module ExportSpecifier>;
pub type Declarations = HashMap<String, EntityDeclaration>;
pub type DeclarationsWithExport<'module> = Vec<&'module ExportDecl>;
pub type ExportsFacadeAll<'module> = Vec<&'module ExportAll>;
pub type ExportsNamed<'module> = Vec<&'module NamedExport>;
pub type DefaultExportDeclaration<'module> = Option<&'module ExportDefaultDecl>;
pub type DefaultExportExpression<'module> = Option<&'module ExportDefaultExpr>;

pub fn get_exports_in_module(
    base_import_path: PathBuf,
    module: Module,
) -> Result<(Option<EntityDeclaration>, Declarations)> {
    let (
        mut declarations,
        declarations_with_export,
        exports_facade_all,
        exports_named,
        default_export_declaration,
        default_export_expression,
    ) = get_items_in_module(&module)?;

    let (mut named_exports, named_facade_exports) = get_named_export_declarations(
        &declarations_with_export,
        &exports_named,
        &mut declarations,
    )?;

    let default_export = get_default_export_declaration(
        &default_export_declaration,
        &default_export_expression,
        declarations,
    )?;

    add_facade_export_all_exports(&base_import_path, exports_facade_all, &mut named_exports)?;
    add_named_facade_exports(&base_import_path, named_facade_exports, &mut named_exports)?;

    Ok((default_export, named_exports))
}

fn get_items_in_module(
    module: &Module,
) -> Result<(
    Declarations,
    DeclarationsWithExport,
    ExportsFacadeAll,
    ExportsNamed,
    DefaultExportDeclaration,
    DefaultExportExpression,
)> {
    let mut declarations = Declarations::new();
    let mut declarations_with_export = DeclarationsWithExport::new();

    let mut exports_facade_all = ExportsFacadeAll::new();
    let mut exports_named = ExportsNamed::new();

    let mut default_export_declaration = None;
    let mut default_export_expression = None;

    for item in module.body.iter() {
        if item.is_stmt() {
            let statement = item.as_stmt().unwrap();

            if statement.is_decl() {
                let declaration = statement.as_decl().unwrap();
                let declaration = if declaration.is_var() {
                    EntityDeclaration::from(declaration.as_var().unwrap().decls.last().unwrap())?
                } else if declaration.is_class() {
                    EntityDeclaration::from(declaration.as_class().unwrap())?
                } else if declaration.is_fn_decl() {
                    EntityDeclaration::from(declaration.as_fn_decl().unwrap())?
                } else if declaration.is_ts_module() {
                    todo!("handle TS module declarations")
                } else if declaration.is_ts_enum() {
                    todo!("handle TS enum declarations")
                } else if declaration.is_ts_interface() {
                    todo!("handle TS interface declarations")
                } else if declaration.is_ts_type_alias() {
                    todo!("handle TS type alias declarations")
                } else {
                    continue;
                };

                declarations.insert(declaration.name().to_owned(), declaration);
            }
        } else if item.is_module_decl() {
            let module_declaration = item.as_module_decl().unwrap();

            if module_declaration.is_export_decl() {
                declarations_with_export.push(module_declaration.as_export_decl().unwrap());
            } else if module_declaration.is_export_default_decl() {
                default_export_declaration = module_declaration.as_export_default_decl();
            } else if module_declaration.is_export_default_expr() {
                default_export_expression = module_declaration.as_export_default_expr();
            } else if module_declaration.is_export_all() {
                exports_facade_all.push(module_declaration.as_export_all().unwrap());
            } else if module_declaration.is_export_named() {
                exports_named.push(module_declaration.as_export_named().unwrap());
            } else if module_declaration.is_ts_export_assignment() {
                todo!("handle TS export assignments")
            } else if module_declaration.is_ts_namespace_export() {
                todo!("handle TS namespace exports")
            }
        }
    }

    Ok((
        declarations,
        declarations_with_export,
        exports_facade_all,
        exports_named,
        default_export_declaration,
        default_export_expression,
    ))
}

fn get_default_export_declaration<'module>(
    default_export_declaration: &DefaultExportDeclaration<'module>,
    default_export_expression: &DefaultExportExpression<'module>,
    mut declarations: Declarations,
) -> Result<Option<EntityDeclaration>> {
    if let Some(export) = default_export_declaration {
        return Ok(Some(EntityDeclaration::from(&export.decl)?));
    } else if default_export_expression.is_none() {
        return Ok(None);
    }

    let expression = default_export_expression.unwrap();
    let export_identity = expression.expr.as_ident().unwrap();
    let export_name = export_identity.sym.to_string();

    if let Some(declaration) = declarations.remove(&export_name) {
        return Ok(Some(declaration));
    }

    Ok(None)
}

fn get_named_export_declarations<'module>(
    declarations_with_export: &DeclarationsWithExport<'module>,
    exports_named: &ExportsNamed<'module>,
    declarations: &mut Declarations,
) -> Result<(Declarations, ExternalSpecifiers<'module>)> {
    let mut internal_exports: Declarations = Declarations::new();
    let mut external_exports: ExternalSpecifiers = ExternalSpecifiers::new();

    for export in declarations_with_export {
        let export_declaration = EntityDeclaration::from(*export)?;
        let export_name = export_declaration.name().to_owned();

        internal_exports.insert(export_name, export_declaration);
    }

    for export in exports_named {
        let external_export_src = &export.src;

        for specifier in export.specifiers.iter() {
            if let Some(import_path) = external_export_src {
                external_exports.insert(PathBuf::from(import_path.value.to_string()), specifier);
                continue;
            }

            if specifier.is_default() {
                // This is likely used internally by SWC as part of their AST, since
                // there's no way to name a default named export. A { default } export
                // is handled by the is_named case above.
                bail!("Cannot handle named default export specifier.")
            }

            if specifier.is_named() {
                let specifier = specifier.as_named().unwrap();
                let (actual_name, exported_name) = get_named_export_names(specifier);
                let exported_declaration = declarations.remove(&actual_name);

                if exported_declaration.is_some() {
                    internal_exports.insert(exported_name, exported_declaration.unwrap());
                }
            }
        }
    }

    Ok((internal_exports, external_exports))
}

fn unwrap_module_export_name(name: &ModuleExportName) -> String {
    match name {
        ModuleExportName::Ident(ident) => ident.sym.to_string(),
        ModuleExportName::Str(name) => name.value.to_string(),
    }
}

fn get_named_export_names(specifier: &ExportNamedSpecifier) -> (String, String) {
    let actual_name = unwrap_module_export_name(&specifier.orig);
    let given_name = specifier.exported.as_ref().map(unwrap_module_export_name);
    let exported_name = given_name.or(Some(actual_name.to_owned())).unwrap();

    (actual_name, exported_name)
}

fn add_facade_export_all_exports(
    base_import_path: &PathBuf,
    exports: ExportsFacadeAll,
    buffer: &mut Declarations,
) -> Result<()> {
    for export in exports {
        let import_file_path = export.src.value.to_string();
        let import_file_path = base_import_path.join(import_file_path);

        let import_module = parse_import(&import_file_path)?;
        let import_module_dir = import_file_path.parent().unwrap().to_path_buf();
        let (_, facade_named_exports) = get_exports_in_module(import_module_dir, import_module)?;

        buffer.extend(facade_named_exports);
    }

    Ok(())
}

fn add_named_facade_exports(
    base_import_path: &PathBuf,
    exports: ExternalSpecifiers,
    buffer: &mut Declarations,
) -> Result<()> {
    for (import_file_path, exported_specifier) in exports {
        let import_file_path = base_import_path.join(import_file_path);

        let import_module = parse_import(&import_file_path)?;
        let import_module_dir = import_file_path.parent().unwrap().to_path_buf();

        let (facade_default_export, mut facade_named_exports) =
            get_exports_in_module(import_module_dir, import_module)?;

        if exported_specifier.is_named() {
            let specifier = exported_specifier.as_named().unwrap();
            let (actual_name, exported_name) = get_named_export_names(specifier);

            if actual_name.eq("default") {
                buffer.insert(exported_name, facade_default_export.unwrap());
            } else {
                buffer.insert(
                    exported_name.to_owned(),
                    facade_named_exports.remove(&actual_name).unwrap(),
                );
            }
        } else if exported_specifier.is_namespace() {
            let specifier = exported_specifier.as_namespace().unwrap();
            let exported_name = unwrap_module_export_name(&specifier.name);

            for (actual_name, declaration) in facade_named_exports {
                buffer.insert(format!("{}.{}", exported_name, actual_name), declaration);
            }
        }
    }

    Ok(())
}

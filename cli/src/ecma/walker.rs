use crate::ecma::entity::EntityDeclaration;
use crate::ecma::parser::parse_import;
use anyhow::{bail, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use swc_ecma_ast::{
    ExportAll, ExportDecl, ExportDefaultDecl, ExportDefaultExpr, Module, ModuleExportName,
    NamedExport,
};

pub type Declarations = HashMap<String, EntityDeclaration>;
pub type DeclarationsWithExport<'module> = Vec<&'module ExportDecl>;
pub type ExportsFacade<'module> = Vec<&'module ExportAll>;
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
        exports_facade,
        exports_named,
        default_export_declaration,
        default_export_expression,
    ) = get_items_in_module(&module)?;

    let mut named_exports = get_named_export_declarations(
        &declarations_with_export,
        &exports_named,
        &mut declarations,
    )?;

    let default_export = get_default_export_declaration(
        &default_export_declaration,
        &default_export_expression,
        declarations,
    )?;

    for facade_export in exports_facade {
        let import_file_path = facade_export.src.value.to_string();
        let import_file_path = base_import_path.join(import_file_path);

        let import_module = parse_import(&import_file_path)?;
        let import_module_dir_path = import_file_path.parent().unwrap().to_path_buf();

        let (_, facade_named_exports) =
            get_exports_in_module(import_module_dir_path, import_module)?;

        named_exports.extend(facade_named_exports);
    }

    Ok((default_export, named_exports))
}

fn get_items_in_module(
    module: &Module,
) -> Result<(
    Declarations,
    DeclarationsWithExport,
    ExportsFacade,
    ExportsNamed,
    DefaultExportDeclaration,
    DefaultExportExpression,
)> {
    let mut declarations: Declarations = Declarations::new();
    let mut declarations_with_export: DeclarationsWithExport = DeclarationsWithExport::new();

    let mut exports_facade: ExportsFacade = ExportsFacade::new();
    let mut exports_named: ExportsNamed = ExportsNamed::new();

    let mut default_export_declaration: DefaultExportDeclaration = None;
    let mut default_export_expression: DefaultExportExpression = None;

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
                exports_facade.push(module_declaration.as_export_all().unwrap());
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
        exports_facade,
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
) -> Result<Declarations> {
    let mut exports: Declarations = Declarations::new();

    for export in declarations_with_export {
        let export_declaration = EntityDeclaration::from(*export)?;
        let export_name = export_declaration.name().to_owned();

        exports.insert(export_name, export_declaration);
    }

    for export in exports_named {
        for specifier in export.specifiers.iter() {
            if specifier.is_named() {
                let specifier = specifier.as_named().unwrap();

                let original_name = match &specifier.orig {
                    ModuleExportName::Ident(ident) => ident.sym.to_string(),
                    ModuleExportName::Str(name) => name.value.to_string(),
                };

                let exported_name = specifier.exported.as_ref().map(|name| match name {
                    ModuleExportName::Ident(ident) => ident.sym.to_string(),
                    ModuleExportName::Str(name) => name.value.to_string(),
                });

                let declaration = declarations.remove(&original_name);
                let name = exported_name.or(Some(original_name)).unwrap();

                if declaration.is_some() {
                    exports.insert(name, declaration.unwrap());
                }
            } else if specifier.is_namespace() {
                todo!("handle named export namespace specifiers")
            } else if specifier.is_default() {
                // This is likely used internally by SWC as part of their AST, since
                // there's no way to name a default named export. A { default } export
                // is handled by the is_named case above.
                bail!("Cannot handle named default export specifier.")
            }
        }
    }

    Ok(exports)
}

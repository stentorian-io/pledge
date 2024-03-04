use crate::ecma::entity::EntityDeclaration;
use anyhow::Result;
use std::collections::HashMap;
use swc_ecma_ast::{
    ExportAll, ExportDecl, ExportDefaultDecl, ExportDefaultExpr, Module, ModuleExportName,
    NamedExport,
};

pub type Declarations<'decl> = HashMap<String, EntityDeclaration<'decl>>;
pub type DeclarationsWithExport<'decl> = Vec<&'decl ExportDecl>;
pub type ExportsFacade<'decl> = Vec<&'decl ExportAll>;
pub type ExportsNamed<'decl> = Vec<&'decl NamedExport>;
pub type DefaultExportDeclaration<'decl> = Option<&'decl ExportDefaultDecl>;
pub type DefaultExportExpression<'decl> = Option<&'decl ExportDefaultExpr>;

pub fn get_exports_in_module(module: &Module) -> Result<(Option<EntityDeclaration>, Declarations)> {
    let (
        mut declarations,
        declarations_with_export,
        // TODO:
        _exports_facade,
        exports_named,
        default_export_declaration,
        default_export_expression,
    ) = get_items_in_module(module)?;

    let named_exports = get_named_export_declarations(
        &declarations_with_export,
        &exports_named,
        &mut declarations,
    )?;

    let default_export = get_default_export_declaration(
        &default_export_declaration,
        &default_export_expression,
        declarations,
    )?;

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
                    todo!()
                } else if declaration.is_ts_enum() {
                    todo!()
                } else if declaration.is_ts_interface() {
                    todo!()
                } else if declaration.is_ts_type_alias() {
                    todo!()
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
                todo!()
            } else if module_declaration.is_ts_namespace_export() {
                todo!()
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

fn get_default_export_declaration<'decl>(
    default_export_declaration: &DefaultExportDeclaration<'decl>,
    default_export_expression: &DefaultExportExpression<'decl>,
    mut declarations: Declarations<'decl>,
) -> Result<Option<EntityDeclaration<'decl>>> {
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

fn get_named_export_declarations<'decl>(
    declarations_with_export: &DeclarationsWithExport<'decl>,
    exports_named: &ExportsNamed<'decl>,
    declarations: &mut Declarations<'decl>,
) -> Result<Declarations<'decl>> {
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
            }
        }
    }

    Ok(exports)
}

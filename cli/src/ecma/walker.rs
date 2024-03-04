use crate::ecma::entity::EntityDeclaration;
use std::collections::HashMap;
use swc_ecma_ast::{
    ExportAll, ExportDecl, ExportDefaultDecl, ExportDefaultExpr, Module, NamedExport,
};

pub type Declarations<'decl> = HashMap<String, EntityDeclaration<'decl>>;
pub type DeclarationsWithExport<'decl> = Vec<&'decl ExportDecl>;
pub type FacadeExports<'decl> = Vec<&'decl ExportAll>;
pub type NamedExports<'decl> = Vec<&'decl NamedExport>;
pub type DefaultExportDeclaration<'decl> = Option<&'decl ExportDefaultDecl>;
pub type DefaultExportExpression<'decl> = Option<&'decl ExportDefaultExpr>;

pub fn get_exports_in_module(module: &Module) -> (Option<EntityDeclaration>, Declarations) {
    let (
        declarations,
        declarations_with_export,
        _exports_facade,
        exports_named,
        default_export_declaration,
        default_export_expression,
    ) = get_items_in_module(module);

    let named_exports =
        get_named_export_declarations(&declarations_with_export, &exports_named, &declarations);

    let default_export = get_default_export_declaration(
        &default_export_declaration,
        &default_export_expression,
        declarations,
    );

    (default_export, named_exports)
}

fn get_items_in_module(
    module: &Module,
) -> (
    Declarations,
    DeclarationsWithExport,
    FacadeExports,
    NamedExports,
    DefaultExportDeclaration,
    DefaultExportExpression,
) {
    let mut declarations: Declarations = Declarations::new();
    let mut declarations_with_export: DeclarationsWithExport = DeclarationsWithExport::new();

    let mut exports_facade: FacadeExports = FacadeExports::new();
    let mut exports_named: NamedExports = NamedExports::new();

    let mut default_export_declaration: DefaultExportDeclaration = None;
    let mut default_export_expression: DefaultExportExpression = None;

    for item in module.body.iter() {
        if item.is_stmt() {
            let statement = item.as_stmt().unwrap();

            if statement.is_decl() {
                let declaration = statement.as_decl().unwrap();
                let declaration = if declaration.is_var() {
                    EntityDeclaration::from(declaration.as_var().unwrap().decls.last().unwrap())
                } else if declaration.is_class() {
                    EntityDeclaration::from(declaration.as_class().unwrap())
                } else if declaration.is_fn_decl() {
                    EntityDeclaration::from(declaration.as_fn_decl().unwrap())
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
            }
        }
    }

    (
        declarations,
        declarations_with_export,
        exports_facade,
        exports_named,
        default_export_declaration,
        default_export_expression,
    )
}

fn get_default_export_declaration<'decl>(
    default_export_declaration: &DefaultExportDeclaration<'decl>,
    default_export_expression: &DefaultExportExpression<'decl>,
    mut declarations: Declarations<'decl>,
) -> Option<EntityDeclaration<'decl>> {
    if let Some(export) = default_export_declaration {
        return Some(EntityDeclaration::from(&export.decl));
    } else if default_export_expression.is_none() {
        return None;
    }

    let expression = default_export_expression.unwrap();
    let export_identity = expression.expr.as_ident().unwrap();
    let export_name = export_identity.sym.to_string();

    let declaration = declarations.remove(&export_name);

    if let Some(declaration) = declaration {
        return Some(declaration);
    }

    None
}

fn get_named_export_declarations<'decl>(
    _declarations_with_export: &DeclarationsWithExport<'decl>,
    _exports_named: &NamedExports<'decl>,
    _declarations: &Declarations<'decl>,
) -> Declarations<'decl> {
    let exports: Declarations = Declarations::new();

    exports
}

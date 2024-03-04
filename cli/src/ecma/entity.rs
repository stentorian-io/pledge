use anyhow::{bail, Result};
use swc_ecma_ast::{Class, ClassDecl, DefaultDecl, ExportDecl, FnDecl, Function, VarDeclarator};

pub enum EntityDeclaration<'decl> {
    Var(String, &'decl VarDeclarator),
    Class(String, &'decl Class),
    Func(String, &'decl Function),
}

pub trait AsEntityDeclaration<'decl> {
    fn from(&self) -> Result<EntityDeclaration<'decl>>;
}

impl<'decl> EntityDeclaration<'decl> {
    pub fn from<Entity: AsEntityDeclaration<'decl>>(
        entity: Entity,
    ) -> Result<EntityDeclaration<'decl>> {
        entity.from()
    }

    pub fn name(&self) -> &String {
        match self {
            EntityDeclaration::Var(name, _) => name,
            EntityDeclaration::Class(name, _) => name,
            EntityDeclaration::Func(name, _) => name,
        }
    }
}

impl<'decl> AsEntityDeclaration<'decl> for &'decl ExportDecl {
    fn from(&self) -> Result<EntityDeclaration<'decl>> {
        if self.decl.is_fn_decl() {
            let declaration = self.decl.as_fn_decl().unwrap();
            let name = declaration.ident.sym.to_string();
            let function = declaration.function.as_ref();

            Ok(EntityDeclaration::Func(name, function))
        } else if self.decl.is_class() {
            let declaration = self.decl.as_class().unwrap();
            let name = declaration.ident.sym.to_string();
            let class = declaration.class.as_ref();

            Ok(EntityDeclaration::Class(name, class))
        } else if self.decl.is_var() {
            let expression = self.decl.as_var().unwrap();
            let declarator = expression.decls.last().unwrap();
            let name = declarator.name.as_ident().unwrap().sym.to_string();

            Ok(EntityDeclaration::Var(name, declarator))
        } else if self.decl.is_ts_interface() {
            todo!()
        } else if self.decl.is_ts_enum() {
            todo!()
        } else if self.decl.is_ts_module() {
            todo!()
        } else if self.decl.is_ts_type_alias() {
            todo!()
        } else if self.decl.is_using() {
            todo!()
        } else {
            bail!("Unsupported ExportDecl entity.")
        }
    }
}

impl<'decl> AsEntityDeclaration<'decl> for &'decl DefaultDecl {
    fn from(&self) -> Result<EntityDeclaration<'decl>> {
        if self.is_fn_expr() {
            let expression = self.as_fn_expr().unwrap();
            let name = expression.ident.as_ref().unwrap().sym.to_string();
            let function = expression.function.as_ref();

            Ok(EntityDeclaration::Func(name, function))
        } else if self.is_class() {
            let expression = self.as_class().unwrap();
            let name = expression.ident.as_ref().unwrap().sym.to_string();
            let class = expression.class.as_ref();

            Ok(EntityDeclaration::Class(name, class))
        } else if self.is_ts_interface_decl() {
            todo!()
        } else {
            bail!("Unsupported DefaultDecl entity.")
        }
    }
}

impl<'decl> AsEntityDeclaration<'decl> for &'decl VarDeclarator {
    fn from(&self) -> Result<EntityDeclaration<'decl>> {
        Ok(EntityDeclaration::Var(
            self.name.as_ident().unwrap().sym.to_string(),
            self,
        ))
    }
}

impl<'decl> AsEntityDeclaration<'decl> for &'decl FnDecl {
    fn from(&self) -> Result<EntityDeclaration<'decl>> {
        Ok(EntityDeclaration::Func(
            self.ident.sym.to_string(),
            self.function.as_ref(),
        ))
    }
}

impl<'decl> AsEntityDeclaration<'decl> for &'decl ClassDecl {
    fn from(&self) -> Result<EntityDeclaration<'decl>> {
        Ok(EntityDeclaration::Class(
            self.ident.sym.to_string(),
            self.class.as_ref(),
        ))
    }
}

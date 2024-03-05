use anyhow::{bail, Result};
use swc_ecma_ast::{Class, ClassDecl, DefaultDecl, ExportDecl, FnDecl, Function, VarDeclarator};

pub enum EntityDeclaration {
    Var(String, VarDeclarator),
    Class(String, Box<Class>),
    Func(String, Box<Function>),
}

pub trait AsEntityDeclaration {
    fn from(self) -> Result<EntityDeclaration>;
}

impl EntityDeclaration {
    pub fn from<Entity: AsEntityDeclaration>(entity: Entity) -> Result<EntityDeclaration> {
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

impl AsEntityDeclaration for &ExportDecl {
    fn from(self) -> Result<EntityDeclaration> {
        if self.decl.is_fn_decl() {
            let declaration = self.decl.as_fn_decl().unwrap();
            let name = declaration.ident.sym.to_string();
            let function = declaration.function.to_owned();

            Ok(EntityDeclaration::Func(name, function))
        } else if self.decl.is_class() {
            let declaration = self.decl.as_class().unwrap();
            let name = declaration.ident.sym.to_string();
            let class = declaration.class.to_owned();

            Ok(EntityDeclaration::Class(name, class))
        } else if self.decl.is_var() {
            let expression = self.decl.as_var().unwrap();
            let declarator = expression.decls.last().unwrap();
            let name = declarator.name.as_ident().unwrap().sym.to_string();

            Ok(EntityDeclaration::Var(name, declarator.to_owned()))
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

impl AsEntityDeclaration for &DefaultDecl {
    fn from(self) -> Result<EntityDeclaration> {
        if self.is_fn_expr() {
            let expression = self.as_fn_expr().unwrap();
            let name = expression.ident.as_ref().unwrap().sym.to_string();
            let function = expression.function.to_owned();

            Ok(EntityDeclaration::Func(name, function))
        } else if self.is_class() {
            let expression = self.as_class().unwrap();
            let name = expression.ident.as_ref().unwrap().sym.to_string();
            let class = expression.class.to_owned();

            Ok(EntityDeclaration::Class(name, class))
        } else if self.is_ts_interface_decl() {
            todo!()
        } else {
            bail!("Unsupported DefaultDecl entity.")
        }
    }
}

impl AsEntityDeclaration for &VarDeclarator {
    fn from(self) -> Result<EntityDeclaration> {
        Ok(EntityDeclaration::Var(
            self.name.as_ident().unwrap().sym.to_string(),
            self.to_owned(),
        ))
    }
}

impl AsEntityDeclaration for &FnDecl {
    fn from(self) -> Result<EntityDeclaration> {
        Ok(EntityDeclaration::Func(
            self.ident.sym.to_string(),
            self.function.to_owned(),
        ))
    }
}

impl AsEntityDeclaration for &ClassDecl {
    fn from(self) -> Result<EntityDeclaration> {
        Ok(EntityDeclaration::Class(
            self.ident.sym.to_string(),
            self.class.to_owned(),
        ))
    }
}

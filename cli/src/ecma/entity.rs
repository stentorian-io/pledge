use swc_ecma_ast::{Class, ClassDecl, DefaultDecl, FnDecl, Function, VarDeclarator};

pub enum EntityDeclaration<'decl> {
    Var(String, &'decl VarDeclarator),
    Class(String, &'decl Class),
    Func(String, &'decl Function),
    Other(String),
}

impl<'decl> EntityDeclaration<'decl> {
    pub fn from<Entity: AsEntityDeclaration<'decl>>(entity: Entity) -> EntityDeclaration<'decl> {
        entity.from()
    }

    pub fn name(&self) -> &String {
        match self {
            EntityDeclaration::Var(name, _) => name,
            EntityDeclaration::Class(name, _) => name,
            EntityDeclaration::Func(name, _) => name,
            EntityDeclaration::Other(name) => name,
        }
    }
}

pub trait AsEntityDeclaration<'decl> {
    fn from(&self) -> EntityDeclaration<'decl>;
}

impl<'decl> AsEntityDeclaration<'decl> for &'decl DefaultDecl {
    fn from(&self) -> EntityDeclaration<'decl> {
        if self.is_fn_expr() {
            let expression = self.as_fn_expr().unwrap();
            let name = expression.ident.as_ref().unwrap().sym.to_string();
            let function = expression.function.as_ref();

            EntityDeclaration::Func(name, function)
        } else if self.is_class() {
            let expression = self.as_class().unwrap();
            let name = expression.ident.as_ref().unwrap().sym.to_string();
            let class = expression.class.as_ref();

            EntityDeclaration::Class(name, class)
        } else {
            // TODO: self.is_ts_interface_decl()
            EntityDeclaration::Other(String::new())
        }
    }
}

impl<'decl> AsEntityDeclaration<'decl> for &'decl VarDeclarator {
    fn from(&self) -> EntityDeclaration<'decl> {
        EntityDeclaration::Var(self.name.as_ident().unwrap().sym.to_string(), self)
    }
}

impl<'decl> AsEntityDeclaration<'decl> for &'decl FnDecl {
    fn from(&self) -> EntityDeclaration<'decl> {
        EntityDeclaration::Func(self.ident.sym.to_string(), &self.function)
    }
}

impl<'decl> AsEntityDeclaration<'decl> for &'decl ClassDecl {
    fn from(&self) -> EntityDeclaration<'decl> {
        EntityDeclaration::Class(self.ident.sym.to_string(), &self.class)
    }
}

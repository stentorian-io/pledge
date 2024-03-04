use swc_ecma_ast::{Class, ClassDecl, DefaultDecl, FnDecl, Function, VarDeclarator};

pub enum EntityDeclaration<'decl> {
    VAR(String, &'decl VarDeclarator),
    CLASS(String, &'decl Class),
    FUNC(String, &'decl Function),
    OTHER(String),
}

impl<'decl> EntityDeclaration<'decl> {
    pub fn from<Entity: AsEntityDeclaration<'decl>>(entity: Entity) -> EntityDeclaration<'decl> {
        entity.from()
    }

    pub fn name(&self) -> &String {
        match self {
            EntityDeclaration::VAR(name, _) => name,
            EntityDeclaration::CLASS(name, _) => name,
            EntityDeclaration::FUNC(name, _) => name,
            EntityDeclaration::OTHER(name) => name,
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

            EntityDeclaration::FUNC(name, function)
        } else if self.is_class() {
            let expression = self.as_class().unwrap();
            let name = expression.ident.as_ref().unwrap().sym.to_string();
            let class = expression.class.as_ref();

            EntityDeclaration::CLASS(name, class)
        } else {
            EntityDeclaration::OTHER(String::new())
        }
    }
}

impl<'decl> AsEntityDeclaration<'decl> for &'decl VarDeclarator {
    fn from(&self) -> EntityDeclaration<'decl> {
        EntityDeclaration::VAR(self.name.as_ident().unwrap().sym.to_string(), self)
    }
}

impl<'decl> AsEntityDeclaration<'decl> for &'decl FnDecl {
    fn from(&self) -> EntityDeclaration<'decl> {
        EntityDeclaration::FUNC(self.ident.sym.to_string(), &self.function)
    }
}

impl<'decl> AsEntityDeclaration<'decl> for &'decl ClassDecl {
    fn from(&self) -> EntityDeclaration<'decl> {
        EntityDeclaration::CLASS(self.ident.sym.to_string(), &self.class)
    }
}

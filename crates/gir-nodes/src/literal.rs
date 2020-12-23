use crate::Type;
#[derive(Clone, Debug)]
pub enum Literal {
    Any,
    None,
    Bool(bool),
    //todo
}

impl Literal {
    pub fn get_type(&self) -> Type {
        match self {
            Literal::Any => Type::Any,
            Literal::None => Type::None,
            Literal::Bool(_) => Type::Bool,
        }
    }
}

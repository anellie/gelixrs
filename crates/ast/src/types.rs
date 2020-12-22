use crate::Type;
use smol_str::SmolStr;

impl Type {
    pub fn get(&self) -> TypeE {
        todo!();
    }
}

pub enum TypeE {
    Ident(SmolStr),
    Value(Type),
    Weak(Type),
    RawPtr(Type),

    Closure {
        params: Vec<Type>,
        ret_type: Option<Type>,
    },

    Generic {
        ident: SmolStr,
        types: Vec<Type>,
    },
}

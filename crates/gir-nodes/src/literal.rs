use crate::Type;
use smol_str::SmolStr;

#[derive(Clone, Debug)]
pub enum Literal {
    Any,
    None,
    Null,
    Bool(bool),

    I8(u8),
    I16(u16),
    I32(u32),
    I64(u64),

    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),

    F32(f32),
    F64(f64),

    String { text: SmolStr, ty: Type },
}

impl Literal {
    pub fn get_type(&self) -> Type {
        match self {
            Literal::Any => Type::Any,
            Literal::None => Type::None,
            Literal::Null => Type::Null,
            Literal::Bool(_) => Type::Bool,
            Literal::I8(_) => Type::I8,
            Literal::I16(_) => Type::I16,
            Literal::I32(_) => Type::I32,
            Literal::I64(_) => Type::I64,
            Literal::U8(_) => Type::U8,
            Literal::U16(_) => Type::U16,
            Literal::U32(_) => Type::U32,
            Literal::U64(_) => Type::U64,
            Literal::F32(_) => Type::F32,
            Literal::F64(_) => Type::F64,
            Literal::String { ty, .. } => ty.clone(),
        }
    }
}

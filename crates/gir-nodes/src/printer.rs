use crate::{
    declaration::{ADTType, Declaration, Function, Variable, Visibility, ADT},
    expression::{CastType, ConcreteMethodGet, Expr, Intrinsic},
    module::Module,
    types::print_type_args,
    Literal,
};
use std::{
    fmt,
    fmt::{Debug, Display, Formatter},
    iter::repeat,
};

const INDENT: usize = 4;
type R = Result<(), fmt::Error>;

impl Display for Module {
    fn fmt(&self, f: &mut Formatter) -> R {
        writeln!(f, "--> {}:", self.path)?;
        writeln!(f, "Used names: ")?;
        for name in &self.used_names {
            writeln!(f, "{} ", name)?;
        }
        writeln!(f, "\n\n")?;
        for ty in self.declarations.values() {
            ty.display(f)?;
            writeln!(f)?;
        }
        Ok(())
    }
}

impl Declaration {
    fn display(&self, f: &mut Formatter) -> R {
        match self {
            Declaration::Function(func) => func.borrow().display(f, 0),
            Declaration::Adt(adt) => adt.borrow().display(f, 0),
        }
    }
}

impl Function {
    fn display(&self, f: &mut Formatter, indent_size: usize) -> R {
        let indent = repeat(' ').take(indent_size).collect::<String>();
        write!(f, "{}{} func {}(", indent, self.visibility, self.name)?;

        let mut params = self.parameters.iter();
        params
            .next()
            .map(|param| write!(f, "{}: {}", param.name, param.ty));
        for param in params {
            write!(f, ", {}: {}", param.name, param.ty,)?;
        }

        writeln!(f, ") -> {} {{", self.ret_type)?;
        for typ in self.type_parameters.iter() {
            writeln!(f, "    {}tyvar {}: {:?}", indent, typ.name, typ.bound)?;
        }
        for (name, var) in &self.variables {
            writeln!(
                f,
                "{}    {} {}: {}",
                indent,
                if var.mutable { "var" } else { "val" },
                name,
                var.ty,
            )?;
        }

        if !self.variables.is_empty() {
            writeln!(f)?;
        }

        let mut display_exprs = |exprs: &[Expr]| {
            for expr in exprs {
                write!(f, "{}    ", indent)?;
                expr.display(f, indent_size + INDENT)?;
                writeln!(f)?;
            }
            Ok(())
        };

        if self.exprs.len() == 1 {
            match &self.exprs[0] {
                Expr::Block(exprs) | Expr::Return(box Expr::Block(exprs)) => display_exprs(&exprs),
                _ => display_exprs(&self.exprs),
            }
        } else {
            display_exprs(&self.exprs)
        }?;
        writeln!(f, "{}}}", indent)
    }
}

impl Debug for Function {
    fn fmt(&self, f: &mut Formatter<'_>) -> R {
        self.display(f, 0)
    }
}

impl ADT {
    fn display(&self, f: &mut Formatter, indent_size: usize) -> R {
        let indent = repeat(' ').take(indent_size).collect::<String>();
        write!(f, "{}{} ", indent, self.visibility)?;
        match self.ty {
            ADTType::Class { .. } => write!(f, "class"),
            ADTType::Interface => write!(f, "interface"),
            ADTType::Enum { .. } => write!(f, "enum"),
            ADTType::EnumCase { ty, .. } => write!(f, "case({:?})", ty),
        }?;
        writeln!(f, " {} {{\n", self.name)?;

        for typ in self.type_parameters.iter() {
            writeln!(f, "    {}tyvar {}: {:?}", indent, typ.name, typ.bound)?;
        }
        for field in self.fields.values() {
            writeln!(
                f,
                "    {}{} {} {}: {}",
                indent,
                field.visibility,
                if field.mutable { "var" } else { "val" },
                field.name,
                field.ty
            )?;
        }
        writeln!(f)?;
        for func in self.constructors.iter().chain(self.methods.values()) {
            func.borrow().display(f, indent_size + INDENT)?;
            writeln!(f)?;
        }
        writeln!(f, "{}}}", indent)
    }
}

impl Debug for ADT {
    fn fmt(&self, f: &mut Formatter<'_>) -> R {
        self.display(f, 0)
    }
}

impl Expr {
    fn display(&self, f: &mut Formatter, indent_size: usize) -> R {
        match self {
            Expr::Block(exprs) => {
                let indent = repeat(' ').take(indent_size).collect::<String>();
                writeln!(f, "{{")?;
                for expr in exprs.iter() {
                    write!(f, "{}    ", indent)?;
                    expr.display(f, indent_size + INDENT)?;
                    writeln!(f)?;
                }
                write!(f, "{}}}", indent)
            }

            Expr::Literal(literal) => write!(f, "{}", literal),

            Expr::Variable(var) => {
                write!(f, "{}", var.get_name())?;
                if let Variable::Function(func) = var {
                    print_type_args(f, func.args())?;
                }
                Ok(())
            }

            Expr::Allocate { ty, .. } => write!(f, "allocate({})", ty),

            Expr::Load { object, field } => {
                object.display(f, indent_size + INDENT)?;
                write!(f, ".{}", field.name)
            }

            Expr::Store {
                location, value, ..
            } => {
                location.display(f, indent_size + INDENT)?;
                write!(f, " = ")?;
                value.display(f, indent_size + INDENT)
            }

            Expr::Binary {
                left,
                operator,
                right,
            } => {
                left.display(f, indent_size + INDENT)?;
                write!(f, " {:?} ", operator)?;
                right.display(f, indent_size + INDENT)
            }

            Expr::Unary { right, operator } => {
                write!(f, "{:?}", operator)?;
                right.display(f, indent_size + INDENT)
            }

            Expr::Call { callee, arguments } => {
                callee.display(f, indent_size + INDENT)?;
                write!(f, "(")?;
                let mut args = arguments.iter();
                args.next()
                    .map(|param| param.display(f, indent_size + INDENT));
                for arg in args {
                    write!(f, ", ")?;
                    arg.display(f, indent_size + INDENT)?;
                }
                writeln!(f, ")")
            }

            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                write!(f, "if (")?;
                condition.display(f, indent_size)?;
                write!(f, ") ")?;
                then_branch.display(f, indent_size)?;
                write!(f, " else ")?;
                else_branch.display(f, indent_size)
            }

            Expr::Switch {
                branches,
                else_branch,
                ..
            } => {
                let indent = repeat(' ').take(indent_size).collect::<String>();
                let indent_inner = repeat(' ').take(indent_size + INDENT).collect::<String>();

                writeln!(f, "switch {{")?;
                for branch in branches {
                    write!(f, "{}", indent_inner)?;
                    branch.0.display(f, indent_size + INDENT)?;
                    write!(f, " => ")?;
                    branch.1.display(f, indent_size + INDENT)?;
                    writeln!(f)?;
                }
                write!(f, "{}else => ", indent_inner)?;
                else_branch.display(f, indent_size + INDENT)?;

                writeln!(f, "\n{}}}", indent)
            }

            Expr::Loop {
                condition,
                body,
                else_branch,
                ..
            } => {
                write!(f, "for (")?;
                condition.display(f, indent_size)?;
                write!(f, ") ")?;
                body.display(f, indent_size)?;
                write!(f, " else ")?;
                else_branch.display(f, indent_size)
            }

            Expr::Break(expr) => {
                write!(f, "break ")?;
                expr.display(f, indent_size)
            }

            Expr::Return(expr) => {
                write!(f, "return ")?;
                expr.display(f, indent_size)
            }

            Expr::Cast { inner, to, method } => {
                write!(f, "cast[{}](", to)?;
                inner.display(f, indent_size)?;
                write!(f, ", {})", method)
            }

            Expr::Closure { function, .. } => {
                write!(f, "closure({})", function.borrow().name)
            }

            Expr::TypeGet(ty) => write!(f, "get_type({})", ty),

            Expr::Intrinsic(int) => write!(f, "intrinsic({})", int),
        }
    }
}

impl Display for Literal {
    fn fmt(&self, f: &mut Formatter<'_>) -> R {
        match self {
            Literal::Any => write!(f, "Any"),
            Literal::None => write!(f, "null"),
            Literal::Bool(b) => write!(f, "{}", b),
            Literal::I8(num) => write!(f, "{}i8", num),
            Literal::I16(num) => write!(f, "{}i16", num),
            Literal::I32(num) => write!(f, "{}i32", num),
            Literal::I64(num) => write!(f, "{}i64", num),
            Literal::U8(num) => write!(f, "{}u8", num),
            Literal::U16(num) => write!(f, "{}u16", num),
            Literal::U32(num) => write!(f, "{}u32", num),
            Literal::U64(num) => write!(f, "{}u64", num),
            Literal::F32(num) => write!(f, "{}f32", num),
            Literal::F64(num) => write!(f, "{}f64", num),
            Literal::String { text, .. } => write!(f, "\"{}\"", text),
        }
    }
}

impl Display for Intrinsic {
    fn fmt(&self, f: &mut Formatter<'_>) -> R {
        match self {
            Intrinsic::DecRc(_) => write!(f, "dec_rc("),
            Intrinsic::IncRc(_) => write!(f, "dec_rc("),
            Intrinsic::Free(_) => write!(f, "free("),
            Intrinsic::IfaceCall { .. } => write!(f, "vcall("),
            Intrinsic::ConcreteMethodGet { .. } => write!(f, "method_of("),
        }?;
        match self {
            Intrinsic::IfaceCall { iface: e, .. }
            | Intrinsic::Free(e)
            | Intrinsic::IncRc(e)
            | Intrinsic::DecRc(e) => e.display(f, 0),
            Intrinsic::ConcreteMethodGet(ConcreteMethodGet {
                index,
                interface,
                iface_method,
            }) => write!(
                f,
                "impl_index = {}, iface = {}, name = {}",
                index,
                interface,
                iface_method.borrow().name
            ),
        }?;
        write!(f, ")")
    }
}

impl Display for CastType {
    fn fmt(&self, f: &mut Formatter<'_>) -> R {
        match self {
            CastType::ToInterface(t) => write!(f, "ToInterface({})", t),
            _ => Debug::fmt(self, f),
        }
    }
}

impl Display for Visibility {
    fn fmt(&self, f: &mut Formatter<'_>) -> R {
        match self {
            Visibility::Private => write!(f, "priv"),
            Visibility::Module => write!(f, "mod"),
            Visibility::Public => write!(f, "public"),
        }
    }
}

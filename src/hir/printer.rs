use crate::hir::nodes::{
    declaration::{ADTType, Declaration, Function, Variable, ADT},
    expression::Expr,
    module::Module,
    types::print_type_args,
};
use std::{
    fmt,
    fmt::{Display, Formatter},
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
        write!(f, "{}func {}(", indent, self.name.lexeme)?;

        let mut params = self.parameters.iter();
        params
            .next()
            .map(|param| write!(f, "{}: {}", param.name.lexeme, param.ty));
        for param in params {
            write!(f, ", {}: {}", param.name.lexeme, param.ty,)?;
        }

        writeln!(f, ") {{")?;
        for typ in self.type_parameters.iter() {
            writeln!(
                f,
                "    {}tyvar {}: {:?}",
                indent, typ.name.lexeme, typ.bound
            )?;
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
        for expr in &self.exprs {
            write!(f, "{}    ", indent)?;
            expr.display(f, indent_size + INDENT)?;
            writeln!(f)?;
        }
        writeln!(f, "{}}}", indent)
    }
}

impl ADT {
    fn display(&self, f: &mut Formatter, indent_size: usize) -> R {
        let indent = repeat(' ').take(indent_size).collect::<String>();
        write!(f, "{}", indent)?;
        match self.ty {
            ADTType::Class { .. } => write!(f, "class"),
            ADTType::Interface => write!(f, "interface"),
            ADTType::Enum { .. } => write!(f, "enum"),
            ADTType::EnumCase { .. } => write!(f, "case"),
        }?;
        writeln!(f, " {} {{\n", self.name.lexeme)?;

        for typ in self.type_parameters.iter() {
            writeln!(
                f,
                "    {}tyvar {}: {:?}",
                indent, typ.name.lexeme, typ.bound
            )?;
        }
        for field in self.fields.values() {
            writeln!(
                f,
                "    {}{} {}: {}",
                indent,
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

impl Expr {
    fn display(&self, f: &mut Formatter, indent_size: usize) -> R {
        match self {
            Expr::Block(exprs) => {
                let indent = repeat(' ').take(indent_size).collect::<String>();
                writeln!(f, "{{")?;
                for expr in exprs.iter() {
                    write!(f, "{}    ", indent)?;
                    expr.display(f, indent_size);
                    writeln!(f)?;
                }
                write!(f, "{}}}\n{}", indent, indent)
            }

            Expr::Literal(literal, _) => write!(f, "{}", literal),

            Expr::Variable(var) => {
                write!(f, "{}", var.get_token().lexeme)?;
                if let Variable::Function(func) = var {
                    print_type_args(f, &func.args)?;
                }
                Ok(())
            }

            Expr::Allocate { ty, .. } => write!(f, "allocate({})", ty),

            Expr::Load { object, field } => {
                object.display(f, indent_size + INDENT)?;
                write!(f, ".{}", field.name)
            }

            Expr::Store {
                location,
                value,
                first_store,
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
                write!(f, " {} ", operator.lexeme)?;
                right.display(f, indent_size + INDENT)
            }

            Expr::Unary { right, operator } => {
                write!(f, "{}", operator.lexeme)?;
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
                phi_type,
            } => {
                write!(f, "if (")?;
                condition.display(f, indent_size + INDENT)?;
                write!(f, ") ")?;
                then_branch.display(f, indent_size + INDENT)?;
                write!(f, " else ")?;
                else_branch.display(f, indent_size + INDENT)
            }

            Expr::Switch {
                branches,
                else_branch,
                phi_type,
            } => {
                let indent = repeat(' ').take(indent_size).collect::<String>();
                let indent_inner = repeat(' ').take(indent_size + INDENT).collect::<String>();

                writeln!(f, "switch {{")?;
                for branch in branches {
                    write!(f, "{}", indent_inner)?;
                    branch.0.display(f, indent_size + INDENT * 2)?;
                    write!(f, " => ")?;
                    branch.1.display(f, indent_size + INDENT * 2)?;
                    writeln!(f)?;
                }
                write!(f, "{}else => ", indent_inner)?;
                else_branch.display(f, indent_size + INDENT * 2)?;

                writeln!(f, "{}}}", indent)
            }

            Expr::Loop {
                condition,
                body,
                else_branch,
                phi_type,
            } => {
                write!(f, "for (")?;
                condition.display(f, indent_size + INDENT)?;
                write!(f, ") ")?;
                body.display(f, indent_size + INDENT)?;
                write!(f, " else ")?;
                else_branch.display(f, indent_size + INDENT)
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
                inner.display(f, indent_size + INDENT)?;
                write!(f, ", {:?})", method)
            }

            Expr::TypeGet(ty) => write!(f, "get_type({})", ty),
        }
    }
}

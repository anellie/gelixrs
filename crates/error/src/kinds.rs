use smol_str::SmolStr;
use strum_macros::*;

use GErr::*;

#[derive(Debug, AsRefStr)]
pub enum GErr {
    // Unexpected parser token
    E001 {
        want: &'static str,
        after: &'static str,
    },
    // Invalid top-level declaration
    E002,
    // Expected type
    E003,
    // Expected ADT decl
    E004,
    // Expected ADT member info
    E005,
    // Invalid modifier
    E006 {
        modifier: String,
        on: &'static str,
    },
    // Multiple else on when
    E007,
    // Expected expression
    E008,

    // Already defined name
    E100(SmolStr),
    // Could not find main function
    E101,

    // Cannot assign to
    E200(&'static str),
    // Mismatched types on assignment
    E201,
    // No implementation of operators
    E202,
    // Cannot call methods in constructors until all ADT members are initialized
    E203,
    // Fields cannot be called
    E204,
    // This variable may not be captured
    E205,
    // Undefined variable
    E206(SmolStr),
    // Break is only allowed in loops
    E207,
    // Cannot redefine variable in same scope
    E208(SmolStr),
    // Break and for must have same type
    E209 {
        expected: String,
        was: String,
    },
    // Unknown field or method
    E210,
    // Can only call generic methods directly
    E211,
    // Return expr was different than function ret type
    E212 {
        expected: String,
        was: String,
    },
    // Cannot have type args on local variable
    E213,
    // Cannot infer types
    E214,
    // Cannot call
    E215(String),
    // Incorrect function parameter count
    E216 {
        expected: usize,
        was: usize,
    },
    // This method requires a strong reference
    E217,
    // Call argument was the wrong type
    E218 {
        expected: String,
        was: String,
    },
    // No matching constructor found for arguments
    E219,
    // (If, For) condition must be a boolean
    E220,
    // Cannot get ADT method
    E221,
    // Cannot get uninitialized ADT member
    E222,
    // Unknown enum case
    E223,
    // Static access is only supported on enum types
    E224,
    // Static access is not supported on values
    E225,
    // 'new' can only be used with constructors
    E226,
    // '!' can only be used on boolean values
    E227,
    // '-' can only be used on signed integers and floats
    E228,
    // Branches of when must be of same type as the value compared
    E229,
    // Cannot assign type to a variable
    E230(String),
    // Unfinished string escape sequence
    E231,
    // Unknown string escape sequence
    E232,
    // Numeric literal does not fit into target type.
    E233,
    // ADT member may not be a weak reference
    E234,
    // ADT member cannot be defined twice
    E235,
    // Cannot have member and method with same name
    E236(SmolStr),

    // Unknown type
    E300(String),
    // Cannot use function as type
    E301,
    // Weak is only applicable to ADTs
    E302,
    // Value is only applicable to ADTs
    E303,
    // Type does not take type arguments
    E304,
    // Can't define main multiple times
    E305,
    // Interface already defined for type
    E306,
    // Only interfaces can be implemented
    E307,
    // Cannot return a weak reference
    E308,
    // Cannot have uninitialized members after constructor
    E309(Vec<SmolStr>),
    // Body type does not match function return type
    E310 {
        expected: String,
        was: String,
    },
}

impl GErr {
    pub fn fmt(&self) -> String {
        match self {
            E001 { want, after } => format!("Expected {} after {}.", want, after),
            E006 { modifier, on } => format!("Cannot have '{:?}' modifier on {}.", modifier, on),

            E100(name) => format!("Name {} already defined in this module", name),

            E200(name) => format!("Cannot assign to {}", name),
            E206(name) => format!("Variable '{}' is not defined", name),
            E208(name) => format!("Cannot redefine variable '{}' in the same scope.", name),
            E209 { expected, was } => format!(
                "Break expressions and for body must have same type (Expected {}, was {}).",
                expected, was
            ),
            E212 { expected, was } => format!(
                "Return expression in function has wrong type (Expected {}, was {}).",
                expected, was
            ),
            E215(thing) => format!("'{}' cannot be called.", thing),
            E216 { expected, was } => format!(
                "Incorrect amount of function arguments. (Expected {}; got {}).",
                expected, was
            ),
            E218 { expected, was } => format!(
                "Call argument is the wrong type (Expected {}, was {}).",
                expected, was
            ),
            E230(ty) => format!("Cannot assign type '{}' to a variable.", ty),
            E236(name) => format!("Cannot have member and method '{}' with same name.", name),

            E300(name) => format!("Unknown type '{}'.", name),

            E309(names) => {
                let mut str = format!(
                    "Cannot have uninitialized fields after constructor (Missing: {}",
                    names[0]
                );
                for name in names.iter().skip(1) {
                    str.push_str(&format!(", {}", name));
                }
                str.push_str(").");
                str
            }
            E310 { expected, was } => format!(
                "Body type does not match function return type (Expected {}, was {}).",
                expected, was
            ),

            _ => self.msg().to_string(),
        }
    }

    fn msg(&self) -> &str {
        match self {
            E002 => "Expected top-level declaration.",
            E003 => "Expected type.",
            E004 => "Encountered invalid declaration inside declaration.",
            E005 => "Expected ':' or '=' after ADT member name.",
            E007 => "'when' expression can only have 1 'else' branch.",
            E008 => "Expected expression.",

            E101 => "Could not find main function.",

            E201 => "Value is a different type than assignment target.",
            E202 => "No implementation of operator found for types.",
            E203 => "Cannot call methods in constructors until all ADT members are initialized.",
            E204 => "Fields cannot be called.",
            E205 => "This variable may not be captured (weak reference)",
            E207 => "Break is only allowed in loops.",
            E210 => "Unknown field or method.",
            E211 => "Can only call generic methods directly.",
            E213 => "Cannot use type arguments on local variables.",
            E214 => "Cannot infer types (please specify explicitly).",
            E217 => "This method requires a strong reference.",
            E219 => "No matching constructor found for arguments.",
            E220 => "Condition must be a boolean.",
            E221 => "Cannot get ADT method (must be called).",
            E222 => "Cannot get uninitialized ADT member.",
            E223 => "Unknown enum case.",
            E224 => "Static access is only supported on enum types.",
            E225 => "Static access is not supported on values.",
            E226 => "'new' can only be used with constructors.",
            E227 => "'!' can only be used on boolean values.",
            E228 => "'-' can only be used on signed integers and floats.",
            E229 => "Branches of when must be of same type as the value compared.",
            E231 => "String escape sequence is unfinished.",
            E232 => "Unknown string escape sequence.",
            E233 => "Numeric literal does not fit into target type.",
            E234 => "ADT member may not be a weak reference.",
            E235 => "ADT member cannot be defined twice.",

            E301 => "Functions cannot be used as types.",
            E302 => "Weak is only applicable to ADTs.",
            E303 => "Value is only applicable to ADTs.",
            E304 => "Type does not take type arguments.",
            E305 => "Can't define main multiple times.",
            E306 => "Interface already defined for type.",
            E307 => "Only interfaces can be implemented.",
            E308 => "Cannot return a weak reference.",

            _ => unreachable!(),
        }
    }
}

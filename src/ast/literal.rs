/// An enum containing all literals possible in Gelix.
#[derive(Debug)]
pub enum Literal {
    None,
    Bool(bool),
    Int(i64),
    Float(f32),
    Double(f64),
    Char(char),
    String(String),
    Array(Vec<Literal>),
}

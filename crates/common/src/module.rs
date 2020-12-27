use smol_str::SmolStr;
use std::{
    fmt::{Display, Error, Formatter},
    rc::Rc,
};

/// The path of a module in the context of a gelix program.
/// For example, the file 'std/collections/array.gel' would have `["std", "collections", "array"]` here.
pub type ModulePath = Rc<ModPath>;

#[derive(Clone, Debug, Default, PartialOrd, PartialEq, Eq, Hash)]
pub struct ModPath(Vec<SmolStr>);

impl ModPath {
    pub fn new() -> Self {
        ModPath(Vec::with_capacity(8))
    }

    pub fn from(path: Vec<SmolStr>) -> Self {
        ModPath(path)
    }

    pub fn is<'t, T: PartialEq<&'t str>>(&'t self, other: &'t [T]) -> bool {
        self.0
            .iter()
            .zip(other.iter())
            .all(|(a, b)| b == &a.as_str())
    }

    pub fn parts(&self) -> &[SmolStr] {
        &self.0
    }

    pub fn index(&self, i: usize) -> Option<&SmolStr> {
        self.0.get(i)
    }

    pub fn push(&mut self, s: SmolStr) {
        self.0.push(s);
    }

    pub fn pop(&mut self) {
        self.0.pop();
    }
}

impl Display for ModPath {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        write!(
            f,
            "{}",
            self.0
                .iter()
                .map(|rc| rc.as_ref())
                .collect::<Vec<&str>>()
                .join("/")
        )
    }
}

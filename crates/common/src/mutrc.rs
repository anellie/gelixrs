use std::{cell::RefCell, rc::Rc};

pub type MutRc<T> = Rc<RefCell<T>>;

pub fn mutrc_new<T>(value: T) -> MutRc<T> {
    Rc::new(RefCell::new(value))
}

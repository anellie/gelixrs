use std::rc::Rc;
use std::cell::RefCell;

pub type MutRc<T> = Rc<RefCell<T>>;

pub fn mutrc_new<T>(value: T) -> MutRc<T> {
    Rc::new(RefCell::new(value))
}
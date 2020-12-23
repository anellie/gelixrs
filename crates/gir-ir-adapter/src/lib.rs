use indexmap::map::IndexMap;
use inkwell::{
    types::StructType,
    values::{FunctionValue, PointerValue},
};
use std::{
    fmt,
    fmt::{Debug, Formatter},
    hash::Hash,
    rc::Rc,
};

pub enum IRAdapter<T: Copy, A: Hash + Eq> {
    NoTypeArgs(Option<T>),
    TypeArgs(IndexMap<Rc<A>, T>),
}

impl<T: Copy, A: Hash + Eq> IRAdapter<T, A> {
    pub fn new(type_args: bool) -> Self {
        if type_args {
            IRAdapter::TypeArgs(IndexMap::with_capacity(3))
        } else {
            IRAdapter::NoTypeArgs(None)
        }
    }

    pub fn get_inst(&self, args: &Rc<A>) -> Option<T> {
        match self {
            IRAdapter::NoTypeArgs(opt) => *opt,
            IRAdapter::TypeArgs(map) => map.get(args).copied(),
        }
    }

    pub fn add_inst(&mut self, args: &Rc<A>, ir: T) {
        match self {
            IRAdapter::NoTypeArgs(opt) => {
                opt.replace(ir);
            }
            IRAdapter::TypeArgs(map) => {
                map.insert(Rc::clone(args), ir);
            }
        };
    }

    pub fn count(&self) -> usize {
        match self {
            IRAdapter::NoTypeArgs(_) => 1,
            IRAdapter::TypeArgs(map) => map.len(),
        }
    }
}

impl<T: Copy, A: Hash + Eq> Debug for IRAdapter<T, A> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "<IR adapter>")
    }
}

impl<'a, T: Copy, A: Hash + Eq> IntoIterator for &'a IRAdapter<T, A> {
    type Item = (&'a T, Option<&'a Rc<A>>);
    type IntoIter = AdapterIter<'a, T, A>;

    fn into_iter(self) -> Self::IntoIter {
        AdapterIter {
            inner: self,
            count: 0,
        }
    }
}

pub struct AdapterIter<'a, T: Copy, A: Hash + Eq> {
    inner: &'a IRAdapter<T, A>,
    count: usize,
}

impl<'a, T: Copy, A: Hash + Eq> Iterator for AdapterIter<'a, T, A> {
    type Item = (&'a T, Option<&'a Rc<A>>);

    fn next(&mut self) -> Option<Self::Item> {
        let ret = match self.inner {
            IRAdapter::NoTypeArgs(item) if self.count == 0 => item.as_ref().map(|i| (i, None)),
            IRAdapter::TypeArgs(map) => map.get_index(self.count).map(|(k, v)| (v, Some(k))),
            _ => None,
        };
        self.count += 1;
        ret
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self.inner {
            IRAdapter::NoTypeArgs(_) => (1, Some(1)),
            IRAdapter::TypeArgs(map) => (map.len(), Some(map.len())),
        }
    }
}

#[derive(Copy, Clone)]
pub struct IRAdtInfo {
    pub strong: StructType,
    pub weak: StructType,
    pub typeinfo: PointerValue,
}

pub type IRFunction<A> = IRAdapter<FunctionValue, A>;
pub type IRAdt<A> = IRAdapter<IRAdtInfo, A>;
pub type IRClosure = StructType;

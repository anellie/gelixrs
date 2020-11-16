use crate::gir::{nodes::types::TypeArguments};
use indexmap::map::IndexMap;
use inkwell::{
    types::StructType,
    values::{FunctionValue, PointerValue},
};
use std::{
    fmt,
    fmt::{Debug, Formatter},
    rc::Rc,
};
use IRAdapter::TypeArgs;

pub enum IRAdapter<T: Copy> {
    NoTypeArgs(Option<T>),
    TypeArgs(IndexMap<Rc<TypeArguments>, T>),
}

impl<T: Copy> IRAdapter<T> {
    pub fn new(type_args: bool) -> Self {
        if type_args {
            IRAdapter::TypeArgs(IndexMap::with_capacity(3))
        } else {
            IRAdapter::NoTypeArgs(None)
        }
    }

    pub fn get_inst(&self, args: &Rc<TypeArguments>) -> Option<T> {
        match self {
            IRAdapter::NoTypeArgs(opt) => *opt,
            IRAdapter::TypeArgs(map) => map.get(args).copied(),
        }
    }

    pub fn add_inst(&mut self, args: &Rc<TypeArguments>, ir: T) {
        match self {
            IRAdapter::NoTypeArgs(opt) => {
                opt.replace(ir);
            },
            IRAdapter::TypeArgs(map) => {
                map.insert(Rc::clone(args), ir);
            },
        };
    }

    pub fn count(&self) -> usize {
        match self {
            IRAdapter::NoTypeArgs(_) => 1,
            IRAdapter::TypeArgs(map) => map.len(),
        }
    }
}

impl<T: Copy> Debug for IRAdapter<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "<IR adapter>")
    }
}

impl<'a, T: Copy> IntoIterator for &'a IRAdapter<T> {
    type Item = (&'a T, Option<&'a Rc<TypeArguments>>);
    type IntoIter = AdapterIter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        AdapterIter {
            inner: self,
            count: 0,
        }
    }
}

pub struct AdapterIter<'a, T: Copy> {
    inner: &'a IRAdapter<T>,
    count: usize,
}

impl<'a, T: Copy> Iterator for AdapterIter<'a, T> {
    type Item = (&'a T, Option<&'a Rc<TypeArguments>>);

    fn next(&mut self) -> Option<Self::Item> {
        let ret = match self.inner {
            IRAdapter::NoTypeArgs(item) if self.count == 0 => item.as_ref().map(|i| (i, None)),
            TypeArgs(map) => map
                .get_index(self.count)
                .map(|(k, v)| (v, Some(k))),
            _ => None,
        };
        self.count += 1;
        ret
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self.inner {
            IRAdapter::NoTypeArgs(_) => (1, Some(1)),
            TypeArgs(map) => (map.len(), Some(map.len())),
        }
    }
}

#[derive(Copy, Clone)]
pub struct IRAdtInfo {
    pub strong: StructType,
    pub weak: StructType,
    pub typeinfo: PointerValue,
}

pub type IRFunction = IRAdapter<FunctionValue>;
pub type IRAdt = IRAdapter<IRAdtInfo>;
pub type IRClosure = StructType;

use inkwell::values::{FunctionValue, StructValue};
use crate::gir::{Type, Function, ADT};
use indexmap::map::IndexMap;
use std::fmt::{Debug, Formatter};
use std::{io, fmt};
use crate::gir::nodes::types::{Instance, TypeArguments};
use inkwell::types::StructType;
use crate::gir::nodes::declaration::ADTType;

pub enum IRAdapter<T> {
    NoTypeArgs(Option<T>),
    TypeArgs(IndexMap<TypeArguments, Option<T>>)
}

impl<T> IRAdapter<T> {
    pub fn new(type_args: bool) -> Self {
        if type_args {
            IRAdapter::TypeArgs(IndexMap::with_capacity(3))
        } else {
            IRAdapter::NoTypeArgs(None)
        }
    }
}

impl<T> Debug for IRAdapter<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "<IR adapter>")
    }
}

impl<'a, T> IntoIterator for &'a IRAdapter<T> {
    type Item = (&'a T, &'a [Type]);
    type IntoIter = AdapterIter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        AdapterIter {
            inner: self,
            count: 0
        }
    }
}

pub struct AdapterIter<'a, T> {
    inner: &'a IRAdapter<T>,
    count: usize
}

impl<'a, T> Iterator for AdapterIter<'a, T> {
    type Item = (&'a T, &'a [Type]);

    fn next(&mut self) -> Option<Self::Item> {
        let ret = match self.inner {
            IRAdapter::NoTypeArgs(item) if self.count == 0 => item.as_ref().map(|i| (i, &[] as &[Type])),
            IRAdapter::TypeArgs(map) => map.get_index(self.count).map(|(k, v)| (v.as_ref().unwrap(), k.as_slice())),
            _ => None
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

pub type IRFunction = IRAdapter<FunctionValue>;
pub type IRAdt = IRAdapter<StructType>;

pub trait Instantiable {
    fn register_instance(&mut self, new: &TypeArguments);
}

impl Instantiable for Function {
    fn register_instance(&mut self, new: &TypeArguments) {
        match &mut self.ir {
            IRAdapter::TypeArgs(map) => {
                if !map.contains_key(new) {
                    map.insert(new.clone(), None);
                }
            },
            _ => ()
        }
    }
}

impl Instantiable for ADT {
    fn register_instance(&mut self, new: &TypeArguments) {
        match &mut self.ir {
            IRAdapter::TypeArgs(map) => {
                if !map.contains_key(new) {
                    map.insert(new.clone(), None);
                }
            },
            _ => ()
        }
    }
}
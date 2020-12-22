use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

#[derive(PartialEq, Eq, Hash, Copy, Clone)]
#[repr(transparent)]
pub struct Id<T>(usize, PhantomData<T>);

pub struct IdRef<'r, T>(&'r T, usize);

impl<'r, T> Deref for IdRef<'r, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.0
    }
}

pub struct IdRefMut<'r, T>(&'r mut T, usize);

impl<'r, T> Deref for IdRefMut<'r, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'r, T> DerefMut for IdRefMut<'r, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Default)]
#[repr(transparent)]
pub struct Arena<T> {
    inner: Vec<T>,
}

impl<T> Arena<T> {
    pub fn alloc(&mut self, new: T) -> Id<T> {
        self.inner.push(new);
        Id(self.inner.len() - 1, PhantomData)
    }

    pub fn i(&self, id: Id<T>) -> IdRef<T> {
        IdRef(&self.inner[id.0], id.0)
    }

    pub fn im(&mut self, id: Id<T>) -> IdRefMut<T> {
        IdRefMut(&mut self.inner[id.0], id.0)
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.inner.iter()
    }

    pub fn id_iter(&self) -> impl Iterator<Item = Id<T>> {
        (0..self.inner.len()).map(|i| Id(i, PhantomData))
    }
}

/*
 * Developed by Felix Ang. (felix.ang@pm.me).
 * Last modified on 10/30/19 8:00 PM.
 * This file is under the Apache 2.0 license. See LICENSE in the root of this repository for details.
 */

/// This trait implements flattening for Option<Option<T>>.
/// This will no longer be needed once https://github.com/rust-lang/rust/pull/60256
/// is stable.
pub trait Flatten<T> {
    fn flatten_(self) -> Option<T>;
}

impl<T> Flatten<T> for Option<Option<T>> {
    #[inline(always)]
    fn flatten_(self) -> Option<T> {
        self.and_then(|t| t)
    }
}

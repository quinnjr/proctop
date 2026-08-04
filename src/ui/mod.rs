//! The ntui half of rtop: components, keybindings, and layout.

use std::sync::Arc;

pub mod app;
pub mod meters;
pub mod selection;
pub mod table;
pub mod theme;

/// A prop payload compared by pointer rather than by value.
///
/// ntui compares props with `PartialEq` to decide whether a subtree needs
/// re-rendering. `Arc<T>`'s own `PartialEq` compares the pointees, so
/// passing the process list down as a plain `Arc<Vec<ProcRow>>` would deep-
/// compare several hundred processes on every frame — more expensive than
/// the render it is trying to skip. Comparing identity instead is both
/// cheaper and more accurate here, because the sampler produces a fresh
/// allocation for every sample.
#[derive(Debug)]
pub struct Shared<T>(Arc<T>);

impl<T> Shared<T> {
    pub fn new(value: T) -> Self {
        Shared(Arc::new(value))
    }
}

impl<T> std::ops::Deref for Shared<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Shared(self.0.clone())
    }
}

impl<T> PartialEq for Shared<T> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl<T: Default> Default for Shared<T> {
    fn default() -> Self {
        Shared::new(T::default())
    }
}

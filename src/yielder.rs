use crate::scope::in_scope;

use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A handle for sending items into the related stream.
#[derive(Debug)]
pub struct Yielder<T> {
    id: u64,
    _marker: PhantomData<fn(T)>,
}

impl<T> Yielder<T> {
    pub(crate) fn new(id: u64) -> Self {
        Self {
            id,
            _marker: PhantomData,
        }
    }

    /// Send a item into the related stream.
    pub fn yield_(&mut self, val: T) -> Yield<'_, T> {
        let place = unsafe { in_scope(self.id) };
        let value = Some(val);
        Yield { place, value }
    }
}

impl<T, E> Yielder<Result<T, E>> {
    /// Send `Ok(val)` into the related stream.
    pub fn yield_ok(&mut self, val: T) -> Yield<'_, Result<T, E>> {
        self.yield_(Ok(val))
    }

    /// Send `Err(err)` into the related stream.
    pub fn yield_err(&mut self, err: E) -> Yield<'_, Result<T, E>> {
        self.yield_(Err(err))
    }
}

#[derive(Debug)]
#[must_use]
pub struct Yield<'a, T> {
    place: &'a mut Option<T>,
    value: Option<T>,
}

impl<T> Future for Yield<'_, T> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        if this.value.is_none() {
            return Poll::Ready(());
        }
        if this.place.is_none() {
            *this.place = this.value.take();
        }
        Poll::Pending
    }
}

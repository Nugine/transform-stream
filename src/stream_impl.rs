use crate::scope::enter_scope;
use crate::{next_id, Yielder};

use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::{FusedStream, Stream};

/// Asynchronous stream of items
#[derive(Debug)]
pub struct AsyncStream<T, G> {
    id: u64,
    done: bool,
    gen: G,
    _marker: PhantomData<T>,
}

impl<T, G> AsyncStream<T, G> {
    /// Constructs an [`AsyncStream`] by a factory function which returns a future.
    pub fn new<F>(f: F) -> Self
    where
        F: FnOnce(Yielder<T>) -> G,
    {
        let id = next_id();
        let gen = f(Yielder::new(id));
        Self {
            id,
            done: false,
            gen,
            _marker: PhantomData,
        }
    }
}

impl<T, G> Stream for AsyncStream<T, G>
where
    G: Future<Output = ()>,
{
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = unsafe { self.get_unchecked_mut() };
        if this.done {
            return Poll::Ready(None);
        }

        let mut place: Option<T> = None;
        enter_scope(this.id, &mut place, || {
            let gen = unsafe { Pin::new_unchecked(&mut this.gen) };
            if let Poll::Ready(()) = gen.poll(cx) {
                this.done = true;
            }
        });

        if place.is_some() {
            return Poll::Ready(place);
        }

        if this.done {
            Poll::Ready(None)
        } else {
            Poll::Pending
        }
    }
}

impl<T, G> FusedStream for AsyncStream<T, G>
where
    G: Future<Output = ()>,
{
    fn is_terminated(&self) -> bool {
        self.done
    }
}

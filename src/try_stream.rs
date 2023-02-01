use crate::scope::enter_scope;
use crate::{next_id, Yielder};

use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::{FusedStream, Stream};

/// Asynchronous stream of results
#[derive(Debug)]
pub struct AsyncTryStream<T, E, G> {
    id: u64,
    done: bool,
    err: Option<E>,
    gen: G,
    _marker: PhantomData<Result<T, E>>,
}

impl<T, E, G> AsyncTryStream<T, E, G> {
    /// Constructs an [`AsyncTryStream`] by a factory function which returns a future.
    pub fn new<F>(f: F) -> Self
    where
        F: FnOnce(Yielder<Result<T, E>>) -> G,
    {
        let id = next_id();
        let gen = f(Yielder::new(id));
        Self {
            id,
            done: false,
            err: None,
            gen,
            _marker: PhantomData,
        }
    }
}

impl<T, E, G> Stream for AsyncTryStream<T, E, G>
where
    G: Future<Output = Result<(), E>>,
{
    type Item = Result<T, E>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = unsafe { self.get_unchecked_mut() };
        if this.done {
            return Poll::Ready(this.err.take().map(Err));
        }

        let mut place: Option<Result<T, E>> = None;
        enter_scope(this.id, &mut place, || {
            let gen = unsafe { Pin::new_unchecked(&mut this.gen) };
            if let Poll::Ready(ret) = gen.poll(cx) {
                this.done = true;
                if let Err(e) = ret {
                    this.err = Some(e)
                }
            }
        });

        if place.is_some() {
            return Poll::Ready(place);
        }

        if this.done {
            return Poll::Ready(this.err.take().map(Err));
        }
        Poll::Pending
    }
}

impl<T, E, G> FusedStream for AsyncTryStream<T, E, G>
where
    G: Future<Output = Result<(), E>>,
{
    fn is_terminated(&self) -> bool {
        self.done && self.err.is_none()
    }
}

//! Lightweight async stream wrapper.
//!
//! # Usage
//! ```
//! use transform_stream::{try_stream, AsyncTryStream};
//! use futures::StreamExt;
//! use std::io;
//!
//! let stream: AsyncTryStream<Vec<u8>, io::Error, _> = try_stream!{
//!     yield_!(vec![b'1', b'2']);
//!     yield_!(vec![b'3', b'4']);
//!     Ok(())
//! };
//!
//! futures::executor::block_on(async {
//!     futures::pin_mut!(stream);
//!     assert_eq!(stream.next().await.unwrap().unwrap(), vec![b'1', b'2']);
//!     assert_eq!(stream.next().await.unwrap().unwrap(), vec![b'3', b'4']);
//!     assert!(stream.next().await.is_none());
//! });
//! ```

#![forbid(unsafe_code)]
#![deny(
    missing_debug_implementations,
    missing_docs,
    clippy::all,
    clippy::cargo
)]

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use atomic_refcell::AtomicRefCell;
use futures_core::future::BoxFuture;
use futures_core::stream::{FusedStream, Stream};
use pin_project_lite::pin_project;

type Slot<T> = Arc<AtomicRefCell<Option<T>>>;

/// A handle for sending items into the related stream.
#[derive(Debug)]
pub struct Yielder<T> {
    slot: Slot<T>,
}

pin_project! {
    struct Yield<'a, T>{
        slot: &'a mut Slot<T>,
        value: Option<T>,
    }
}

impl<T> Future for Yield<'_, T> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let value: &mut Option<T> = this.value;
        let mut slot_guard = this.slot.borrow_mut();
        let slot: &mut Option<T> = &mut *slot_guard;

        if value.is_none() {
            return Poll::Ready(());
        }

        if slot.is_none() {
            *slot = value.take()
        }

        Poll::Pending
    }
}

impl<T> Yielder<T> {
    /// Send a item into the related stream.
    pub async fn yield_item(&mut self, item: T) {
        Yield {
            slot: &mut self.slot,
            value: Some(item),
        }
        .await
    }

    /// Send items into the related stream.
    pub async fn yield_iter<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = T>,
    {
        for item in iter {
            self.yield_item(item).await
        }
    }
}

impl<T, E> Yielder<Result<T, E>> {
    /// Send `Ok(value)` into the related stream.
    pub async fn yield_ok(&mut self, value: T) {
        self.yield_item(Ok(value)).await
    }

    /// Send ok values into the related stream.
    pub async fn yield_ok_iter<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = T>,
    {
        for value in iter {
            self.yield_item(Ok(value)).await
        }
    }
}

fn make_pair<T>() -> (Slot<T>, Slot<T>) {
    let rx = Arc::new(AtomicRefCell::new(None));
    let tx = Arc::clone(&rx);
    (rx, tx)
}

pin_project! {
    /// Asynchronous stream of items
    pub struct AsyncStream<T, G = BoxFuture<'static, ()>> {
        slot: Slot<T>,
        done: bool,
        #[pin]
        gen: G,
    }
}

impl<T, G> AsyncStream<T, G>
where
    G: Future<Output = ()>,
{
    /// Constructs an `AsyncStream` by a factory function which returns a future.
    pub fn new<F>(f: F) -> Self
    where
        F: FnOnce(Yielder<T>) -> G,
    {
        let (rx, tx) = make_pair();
        let yielder = Yielder { slot: tx };
        let gen = f(yielder);
        Self {
            slot: rx,
            gen,
            done: false,
        }
    }
}

impl<'a, T> AsyncStream<T, BoxFuture<'a, ()>> {
    /// Constructs an `AsyncStream` by a factory function which returns a future.
    ///
    /// The `G` is wrapped as an owned dynamically typed `Future` which allows you to write the type.
    pub fn new_boxed<F, G>(f: F) -> Self
    where
        F: FnOnce(Yielder<T>) -> G,
        G: Future<Output = ()> + Send + 'a,
    {
        let (rx, tx) = make_pair();
        let yielder = Yielder { slot: tx };
        let gen = Box::pin(f(yielder));
        Self {
            slot: rx,
            gen,
            done: false,
        }
    }
}

impl<T, G> Stream for AsyncStream<T, G>
where
    G: Future<Output = ()>,
{
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        if *this.done {
            return Poll::Ready(None);
        }

        debug_assert!(this.slot.borrow().is_none(), "async stream logic error");

        if let Poll::Ready(()) = this.gen.as_mut().poll(cx) {
            *this.done = true;
        }

        {
            let mut slot_guard = this.slot.borrow_mut();
            let slot: &mut Option<T> = &mut *slot_guard;
            if let Some(item) = slot.take() {
                return Poll::Ready(Some(item));
            }
        }

        if *this.done {
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

pin_project! {
    /// Asynchronous stream of results
    pub struct AsyncTryStream<T, E, G = BoxFuture<'static, Result<(), E>>> {
        #[pin]
        inner: AsyncStream<Result<T,E>, G>
    }
}

impl<T, E, G> AsyncTryStream<T, E, G>
where
    G: Future<Output = Result<(), E>>,
{
    /// Constructs an `AsyncTryStream` by a factory function which returns a future.
    pub fn new<F>(f: F) -> Self
    where
        F: FnOnce(Yielder<Result<T, E>>) -> G,
    {
        let (rx, tx) = make_pair();
        let yielder = Yielder { slot: tx };
        let gen = f(yielder);
        Self {
            inner: AsyncStream {
                slot: rx,
                gen,
                done: false,
            },
        }
    }
}

impl<'a, T, E> AsyncTryStream<T, E, BoxFuture<'a, Result<(), E>>> {
    /// Constructs an `AsyncTryStream` by a factory function which returns a future.
    ///
    /// The `G` is wrapped as an owned dynamically typed `Future` which allows you to write the type.
    pub fn new_boxed<F, G>(f: F) -> Self
    where
        F: FnOnce(Yielder<Result<T, E>>) -> G,
        G: Future<Output = Result<(), E>> + Send + 'a,
        E: 'a,
    {
        let (rx, tx) = make_pair();
        let yielder = Yielder { slot: tx };
        let gen = Box::pin(f(yielder));
        Self {
            inner: AsyncStream {
                slot: rx,
                gen,
                done: false,
            },
        }
    }
}

impl<T, E, G> Stream for AsyncTryStream<T, E, G>
where
    G: Future<Output = Result<(), E>>,
{
    type Item = Result<T, E>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project().inner.project();

        if *this.done {
            return Poll::Ready(None);
        }

        debug_assert!(this.slot.borrow().is_none(), "async stream logic error");

        if let Poll::Ready(ret) = this.gen.as_mut().poll(cx) {
            *this.done = true;

            if let Err(e) = ret {
                debug_assert!(this.slot.borrow().is_none(), "async stream logic error");
                return Poll::Ready(Some(Err(e)));
            }
        }

        {
            let mut slot_guard = this.slot.borrow_mut();
            let slot: &mut Option<Result<T, E>> = &mut *slot_guard;
            if let Some(item) = slot.take() {
                return Poll::Ready(Some(item));
            }
        }

        if *this.done {
            Poll::Ready(None)
        } else {
            Poll::Pending
        }
    }
}

impl<T, E, G> FusedStream for AsyncTryStream<T, E, G>
where
    G: Future<Output = Result<(), E>>,
{
    fn is_terminated(&self) -> bool {
        self.inner.done
    }
}

/// Create a new stream
#[macro_export]
macro_rules! stream {
    {$($block:tt)+} => {
        AsyncStream::new(|mut __y| async move{
            macro_rules! yield_ {
                ($v:expr) => {
                    __y.yield_item($v).await
                };
            }

            $($block)+
        })
    }
}

/// Create a new try stream
#[macro_export]
macro_rules! try_stream{
    {$($block:tt)+} => {
        AsyncTryStream::new(|mut __y| async move{
            macro_rules! yield_ {
                ($v:expr) => {
                    __y.yield_ok($v).await
                };
            }

            $($block)+
        })
    }
}

/// Create a new boxed stream
#[macro_export]
macro_rules! boxed_stream {
    {$($block:tt)+} => {
        AsyncStream::new_boxed(|mut __y| async move{
            macro_rules! yield_ {
                ($v:expr) => {
                    __y.yield_item($v).await
                };
            }

            $($block)+
        })
    }
}

/// Create a new boxed try stream
#[macro_export]
macro_rules! boxed_try_stream{
    {$($block:tt)+} => {
        AsyncTryStream::new_boxed(|mut __y| async move{
            macro_rules! yield_ {
                ($v:expr) => {
                    __y.yield_ok($v).await
                };
            }

            $($block)+
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_stream() {
        use futures::{pin_mut, StreamExt};
        use memchr::memchr;
        use std::io;
        use std::mem;

        let bytes: &[&[u8]] = &[b"12", b"34", b"5\n", b"67", b"89", b"10\n", b"11"];
        let io_bytes: Vec<io::Result<Vec<u8>>> = bytes.iter().map(|&b| Ok(Vec::from(b))).collect();

        let source_stream = futures::stream::iter(io_bytes);

        let line_stream: AsyncTryStream<Vec<u8>, io::Error, _> = try_stream! {
            pin_mut!(source_stream);

            let mut buf: Vec<u8> = Vec::new();
            loop {
                match source_stream.next().await {
                    None => break,
                    Some(Err(e)) => return Err(e),
                    Some(Ok(bytes)) => {
                        if let Some(idx) = memchr(b'\n', &bytes) {
                            let pos = idx + 1 + buf.len();
                            buf.extend(bytes);
                            let remaining = buf.split_off(pos);
                            let line = mem::replace(&mut buf, remaining);
                            yield_!(line);
                        } else {
                            buf.extend(bytes);
                        }
                    }
                }
            }

            if !buf.is_empty() {
                yield_!(buf);
            }

            Ok(())
        };

        futures::executor::block_on(async {
            pin_mut!(line_stream);

            let line = line_stream.next().await.unwrap().unwrap();
            assert_eq!(line, b"12345\n");

            let line = line_stream.next().await.unwrap().unwrap();
            assert_eq!(line, b"678910\n");

            let line = line_stream.next().await.unwrap().unwrap();
            assert_eq!(line, b"11");

            assert!(line_stream.next().await.is_none());
            assert_eq!(line_stream.is_terminated(), true);

            assert!(line_stream.next().await.is_none());
            assert!(line_stream.next().await.is_none());
        });
    }

    macro_rules! require_by_ref {
        ($value:expr, $($bound:tt)+) => {{
            fn __require<T: $($bound)+>(_: &T) {}
            __require(&$value);
        }};
    }

    #[test]
    fn markers() {
        use futures::future;
        use std::io;

        let stream = AsyncTryStream::new(|mut y| async move {
            y.yield_ok(1_usize).await;
            io::Result::Ok(())
        });

        require_by_ref!(stream, Send + Sync + 'static);

        let stream_boxed: AsyncTryStream<usize, io::Error> =
            AsyncTryStream::new_boxed(|mut y| async move {
                y.yield_ok(1_usize).await;
                io::Result::Ok(())
            });

        require_by_ref!(stream_boxed, Send + Unpin + 'static);

        type FullMarkerBoxFuture<'a, T> = Box<dyn Future<Output = T> + Send + Sync + Unpin + 'a>;

        let stream_full: AsyncTryStream<
            usize,
            io::Error,
            FullMarkerBoxFuture<'static, io::Result<()>>,
        > = AsyncTryStream::new(|_| -> FullMarkerBoxFuture<'static, io::Result<()>> {
            Box::new(future::ready(io::Result::Ok(())))
        });

        require_by_ref!(stream_full, Send + Sync + Unpin + 'static)
    }

    #[test]
    fn inf() {
        use futures::{pin_mut, StreamExt};

        let stream = stream! {
            for i in 0_i32.. {
                yield_!(i);
            }
        };

        futures::executor::block_on(async move {
            pin_mut!(stream);

            assert_eq!(stream.next().await.unwrap(), 0);
            assert_eq!(stream.next().await.unwrap(), 1);
            assert_eq!(stream.next().await.unwrap(), 2);

            assert_eq!(stream.is_terminated(), false);
        })
    }
}

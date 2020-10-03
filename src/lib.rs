#![forbid(unsafe_code)]

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use atomic_refcell::AtomicRefCell;
use futures_core::future::BoxFuture;
use futures_core::stream::{FusedStream, Stream};
use pin_project_lite::pin_project;

type Channel<T> = Arc<AtomicRefCell<VecDeque<T>>>;

pub struct Yielder<T> {
    tx: Channel<T>,
}

impl<T> Yielder<T> {
    pub async fn yield_item(&mut self, value: T) {
        self.tx.borrow_mut().push_back(value);
    }

    pub async fn yield_iter<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = T>,
    {
        self.tx.borrow_mut().extend(iter.into_iter());
    }
}

impl<T, E> Yielder<Result<T, E>> {
    pub async fn yield_ok(&mut self, value: T) {
        self.tx.borrow_mut().push_back(Ok(value));
    }

    pub async fn yield_ok_iter<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = T>,
    {
        self.tx.borrow_mut().extend(iter.into_iter().map(Ok));
    }
}

pin_project! {
    pub struct AsyncStream<T, G = BoxFuture<'static, ()>> {
        chan: Channel<T>,
        done: bool,
        #[pin]
        gen: G,
    }
}

impl<T, G> AsyncStream<T, G>
where
    G: Future<Output = ()>,
{
    pub fn new<F>(f: F) -> Self
    where
        F: FnOnce(Yielder<T>) -> G,
    {
        let chan = Arc::new(AtomicRefCell::new(VecDeque::new()));
        let tx = Arc::clone(&chan);
        let yielder = Yielder { tx };
        let gen = f(yielder);
        Self {
            chan,
            gen,
            done: false,
        }
    }
}

impl<'a, T> AsyncStream<T, BoxFuture<'a, ()>> {
    pub fn new_boxed<F, G>(f: F) -> Self
    where
        F: FnOnce(Yielder<T>) -> G,
        G: Future<Output = ()> + Send + 'a,
    {
        let chan = Arc::new(AtomicRefCell::new(VecDeque::new()));
        let tx = Arc::clone(&chan);
        let yielder = Yielder { tx };
        let gen = Box::pin(f(yielder));
        Self {
            chan,
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

        loop {
            if let Some(item) = this.chan.borrow_mut().pop_front() {
                return Poll::Ready(Some(item));
            }

            if *this.done {
                return Poll::Ready(None);
            }

            match this.gen.as_mut().poll(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(()) => *this.done = true,
            }
        }
    }
}

impl<T, G> FusedStream for AsyncStream<T, G>
where
    G: Future<Output = ()>,
{
    fn is_terminated(&self) -> bool {
        self.done && self.chan.borrow().is_empty()
    }
}

pin_project! {
    pub struct AsyncTryStream<T, E, G = BoxFuture<'static, Result<(), E>>> {
        #[pin]
        inner: AsyncStream<Result<T,E>,G>
    }
}

impl<T, E, G> AsyncTryStream<T, E, G>
where
    G: Future<Output = Result<(), E>>,
{
    pub fn new<F>(f: F) -> Self
    where
        F: FnOnce(Yielder<Result<T, E>>) -> G,
    {
        let chan = Arc::new(AtomicRefCell::new(VecDeque::new()));
        let tx = Arc::clone(&chan);
        let yielder = Yielder { tx };
        let gen = f(yielder);
        Self {
            inner: AsyncStream {
                chan,
                gen,
                done: false,
            },
        }
    }
}

impl<'a, T, E> AsyncTryStream<T, E, BoxFuture<'a, Result<(), E>>> {
    pub fn new_boxed<F, G>(f: F) -> Self
    where
        F: FnOnce(Yielder<Result<T, E>>) -> G,
        G: Future<Output = Result<(), E>> + Send + 'a,
        E: 'a,
    {
        let chan = Arc::new(AtomicRefCell::new(VecDeque::new()));
        let tx = Arc::clone(&chan);
        let yielder = Yielder { tx };
        let gen = Box::pin(f(yielder));
        Self {
            inner: AsyncStream {
                chan,
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

        loop {
            if let Some(item) = this.chan.borrow_mut().pop_front() {
                return Poll::Ready(Some(item));
            }

            if *this.done {
                return Poll::Ready(None);
            }

            match this.gen.as_mut().poll(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(ret) => {
                    *this.done = true;
                    if let Err(e) = ret {
                        this.chan.borrow_mut().push_back(Err(e));
                    }
                }
            }
        }
    }
}

impl<T, E, G> FusedStream for AsyncTryStream<T, E, G>
where
    G: Future<Output = Result<(), E>>,
{
    fn is_terminated(&self) -> bool {
        self.inner.done && self.inner.chan.borrow().is_empty()
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

        let line_stream: AsyncTryStream<Vec<u8>, io::Error> =
            AsyncTryStream::new_boxed(|mut y| async move {
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
                                y.yield_ok(line).await;
                            }
                        }
                    }
                }

                if !buf.is_empty() {
                    y.yield_ok(buf).await;
                }

                Ok(())
            });

        futures::executor::block_on(async {
            pin_mut!(line_stream);

            while let Some(bytes) = line_stream.next().await {
                let bytes = bytes.unwrap();
                let line = std::str::from_utf8(&bytes).unwrap();
                dbg!(line);
            }
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
}

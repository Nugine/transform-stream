mod stream;

use std::io;
use std::mem;
use std::pin::Pin;

use transform_stream::{try_stream, AsyncTryStream};

use futures_core::{stream::BoxStream, FusedStream, Stream};
use futures_executor::block_on;
use futures_util::{pin_mut, StreamExt};

#[test]
fn line_stream() {
    let bytes: &[&[u8]] = &[b"12", b"34", b"5\n", b"67", b"89", b"10\n", b"11"];
    let io_bytes: Vec<io::Result<Vec<u8>>> = bytes.iter().map(|&b| Ok(Vec::from(b))).collect();

    let source_stream = futures_util::stream::iter(io_bytes);

    let line_stream: AsyncTryStream<Vec<u8>, io::Error, _> =
        AsyncTryStream::new(|mut y| async move {
            pin_mut!(source_stream);

            let mut buf: Vec<u8> = Vec::new();
            loop {
                match source_stream.next().await {
                    None => break,
                    Some(Err(e)) => return Err(e),
                    Some(Ok(bytes)) => {
                        if let Some(idx) = bytes.iter().position(|&b| b == b'\n') {
                            let pos = idx + 1 + buf.len();
                            buf.extend(bytes);
                            let remaining = buf.split_off(pos);
                            let line = mem::replace(&mut buf, remaining);
                            y.yield_ok(line).await;
                        } else {
                            buf.extend(bytes);
                        }
                    }
                }
            }

            if !buf.is_empty() {
                y.yield_ok(buf).await;
            }

            Ok(())
        });

    block_on(async {
        pin_mut!(line_stream);

        let line = line_stream.next().await.unwrap().unwrap();
        assert_eq!(line, b"12345\n");

        let line = line_stream.next().await.unwrap().unwrap();
        assert_eq!(line, b"678910\n");

        let line = line_stream.next().await.unwrap().unwrap();
        assert_eq!(line, b"11");

        assert!(line_stream.next().await.is_none());
        assert!(line_stream.is_terminated());

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
    use futures_util::future;

    fn get_stream() -> impl Stream<Item = io::Result<usize>> {
        try_stream! {
            yield_!(1);
            Ok(())
        }
    }

    let stream = get_stream();
    require_by_ref!(stream, Send + Sync + 'static);

    let stream_boxed: BoxStream<io::Result<usize>> = Box::pin(try_stream! {
        yield_!(1_usize);
        io::Result::Ok(())
    });

    require_by_ref!(stream_boxed, Send + Unpin + 'static);

    type PerfectStream<'a, T> = Pin<Box<dyn Stream<Item = T> + Send + Sync + Unpin + 'a>>;

    let stream_perfect: PerfectStream<'static, io::Result<usize>> =
        Box::pin(AsyncTryStream::new(|_| {
            future::ready(io::Result::<()>::Ok(()))
        }));

    require_by_ref!(stream_perfect, Send + Sync + Unpin + 'static)
}

# transform-stream

[![Crates.io][crates-badge]][crates-url]
[![MIT licensed][mit-badge]][mit-url]
[![Docs][docs-badge]][docs-url]
![CI][ci-badge]
[![Unsafe Forbidden][unsafe-forbidden-badge]][unsafe-forbidden-url]

[crates-badge]: https://img.shields.io/crates/v/transform-stream.svg
[crates-url]: https://crates.io/crates/transform-stream
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: LICENSE
[docs-badge]: https://docs.rs/transform-stream/badge.svg
[docs-url]: https://docs.rs/transform-stream/
[ci-badge]: https://github.com/Nugine/transform-stream/workflows/CI/badge.svg
[unsafe-forbidden-badge]: https://img.shields.io/badge/unsafe-forbidden-success.svg
[unsafe-forbidden-url]: https://github.com/rust-secure-code/safety-dance/

Lightweight async stream wrapper.

## Usage

```rust
use transform_stream::AsyncTryStream;
use futures::StreamExt;
use std::io;

let stream: AsyncTryStream<Vec<u8>, io::Error, _> = AsyncTryStream::new(|mut y| async move {
    y.yield_ok(vec![b'1', b'2']).await;
    y.yield_ok(vec![b'3', b'4']).await;
    Ok(())
});

futures::executor::block_on(async {
    futures::pin_mut!(stream);
    assert_eq!(stream.next().await.unwrap().unwrap(), vec![b'1', b'2']);
    assert_eq!(stream.next().await.unwrap().unwrap(), vec![b'3', b'4']);
    assert!(stream.next().await.is_none());
});
```

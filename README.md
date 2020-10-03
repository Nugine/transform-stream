# transform-stream

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

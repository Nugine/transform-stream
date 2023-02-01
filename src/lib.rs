//! Lightweight async stream wrapper.
//!
//! Inspired by <https://github.com/tokio-rs/async-stream>
//!
//! # Usage
//! ```
//! use transform_stream::{try_stream, AsyncTryStream};
//! use futures_util::{pin_mut, StreamExt};
//! use std::io;
//!
//! let stream: AsyncTryStream<Vec<u8>, io::Error, _> = try_stream!{
//!     yield_!(vec![b'1', b'2']);
//!     yield_!(vec![b'3', b'4']);
//!     Ok(())
//! };
//!
//! futures_executor::block_on(async {
//!     pin_mut!(stream);
//!     assert_eq!(stream.next().await.unwrap().unwrap(), vec![b'1', b'2']);
//!     assert_eq!(stream.next().await.unwrap().unwrap(), vec![b'3', b'4']);
//!     assert!(stream.next().await.is_none());
//! });
//! ```

#![deny(
    missing_debug_implementations,
    missing_docs,
    clippy::all,
    clippy::cargo
)]

mod scope;
mod stream;
mod try_stream;
mod yielder;

pub use self::stream::AsyncStream;
pub use self::try_stream::AsyncTryStream;
pub use self::yielder::Yielder;

pub(crate) fn next_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static ID: AtomicU64 = AtomicU64::new(1);
    ID.fetch_add(1, Ordering::Relaxed)
}

/// Create a new stream
#[macro_export]
macro_rules! stream {
    {$($block:tt)*} => {
        $crate::AsyncStream::new(|mut __y| async move{
            #[allow(unused_macros)]
            macro_rules! yield_ {
                ($v:expr) => {
                    __y.yield_($v).await
                };
            }

            $($block)*
        })
    }
}

/// Create a new try stream
#[macro_export]
macro_rules! try_stream{
    {$($block:tt)*} => {
        $crate::AsyncTryStream::new(|mut __y| async move{
            macro_rules! yield_ {
                ($v:expr) => {
                    __y.yield_ok($v).await
                };
            }

            $($block)*
        })
    }
}

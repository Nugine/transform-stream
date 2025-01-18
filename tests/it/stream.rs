use transform_stream::stream;

use futures_core::FusedStream;
use futures_executor::block_on;
use futures_util::{pin_mut, StreamExt};

#[allow(clippy::never_loop)]
#[test]
fn nop() {
    block_on(async {
        let s = stream! {};
        pin_mut!(s);
        while let Some(()) = s.next().await {
            unreachable!()
        }
    });

    block_on(async {
        let mut flag = false;
        {
            let s = {
                let flag = &mut flag;
                stream! {
                    *flag=true;
                }
            };
            pin_mut!(s);
            while let Some(()) = s.next().await {
                unreachable!()
            }
        }
        assert!(flag);
    })
}

#[test]
fn single() {
    block_on(async {
        let s = stream! {
            yield_!("hello");
        };
        pin_mut!(s);
        assert!(!s.is_terminated());
        assert_eq!(s.next().await, Some("hello"));
        assert_eq!(s.next().await, None);
        assert!(s.is_terminated());
        assert_eq!(s.next().await, None);
    })
}

#[test]
fn infinity() {
    let stream = stream! {
        for i in 0_i32.. {
            yield_!(i);
        }
    };

    block_on(async move {
        pin_mut!(stream);

        assert_eq!(stream.next().await.unwrap(), 0);
        assert_eq!(stream.next().await.unwrap(), 1);
        assert_eq!(stream.next().await.unwrap(), 2);

        assert!(!stream.is_terminated());
    })
}

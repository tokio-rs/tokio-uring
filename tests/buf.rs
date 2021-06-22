/*
use tokio_uring::buf::IoBuf;

const DATA: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789!?";

macro_rules! test_buf {
    (
        $( $name:ident => $buf:expr; )*
    ) => {
        $(
            mod $name {
                use super::*;

                #[test]
                fn test_deref() {
                    let buf = $buf;
                    assert_eq!(&buf[..], DATA);
                }

                #[test]
                fn test_slice_read() {
                    let buf = $buf;

                    let slice = buf.slice(..);
                    assert_eq!(slice.begin(), 0);
                    assert_eq!(slice.end(), DATA.len());

                    assert_eq!(&slice[..], DATA);
                    assert_eq!(&slice[5..], &DATA[5..]);
                    assert_eq!(&slice[10..15], &DATA[10..15]);
                    assert_eq!(&slice[..15], &DATA[..15]);

                    let buf = slice.into_inner();

                    let slice = buf.slice(10..);
                    assert_eq!(slice.begin(), 10);
                    assert_eq!(slice.end(), DATA.len());

                    assert_eq!(&slice[..], &DATA[10..]);
                    assert_eq!(&slice[10..], &DATA[20..]);
                    assert_eq!(&slice[5..15], &DATA[15..25]);
                    assert_eq!(&slice[..15], &DATA[10..25]);

                    let buf = slice.into_inner();

                    let slice = buf.slice(5..15);
                    assert_eq!(slice.begin(), 5);
                    assert_eq!(slice.end(), 15);

                    assert_eq!(&slice[..], &DATA[5..15]);
                    assert_eq!(&slice[5..], &DATA[10..15]);
                    assert_eq!(&slice[5..8], &DATA[10..13]);
                    assert_eq!(&slice[..5], &DATA[5..10]);
                    let buf = slice.into_inner();

                    let slice = buf.slice(..15);
                    assert_eq!(slice.begin(), 0);
                    assert_eq!(slice.end(), 15);

                    assert_eq!(&slice[..], &DATA[..15]);
                    assert_eq!(&slice[5..], &DATA[5..15]);
                    assert_eq!(&slice[5..10], &DATA[5..10]);
                    assert_eq!(&slice[..5], &DATA[..5]);
                }

                #[test]
                fn test_slice_change() {
                    let buf = $buf;
                    let mut slice = buf.slice(..);

                    slice.set_begin(10);
                    assert_eq!(&slice[..], &DATA[10..]);

                    slice.set_end(20);
                    assert_eq!(&slice[..], &DATA[10..20]);
                }
            }
        )*
    };
}

test_buf! {
    vec => IoBuf::from_vec(Vec::from(DATA));
}
*/

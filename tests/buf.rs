use tokio_uring::buf::{IntoSlice, IoBuf, IoBufMut, Slice};

use std::mem;
use std::ops::RangeBounds;
use std::slice::SliceIndex;

#[test]
fn test_vec() {
    let mut v = vec![];

    assert_eq!(v.as_ptr(), v.stable_ptr());
    assert_eq!(v.as_mut_ptr(), v.stable_mut_ptr());
    assert_eq!(v.bytes_init(), 0);
    assert_eq!(v.bytes_total(), 0);

    v.reserve(100);

    assert_eq!(v.as_ptr(), v.stable_ptr());
    assert_eq!(v.as_mut_ptr(), v.stable_mut_ptr());
    assert_eq!(v.bytes_init(), 0);
    assert_eq!(v.bytes_total(), v.capacity());

    v.extend(b"hello");

    assert_eq!(v.as_ptr(), v.stable_ptr());
    assert_eq!(v.as_mut_ptr(), v.stable_mut_ptr());
    assert_eq!(v.bytes_init(), 5);
    assert_eq!(v.bytes_total(), v.capacity());

    // Assume init does not go backwards
    unsafe {
        v.set_init(3);
    }
    assert_eq!(&v[..], b"hello");

    // Initializing goes forward
    unsafe {
        std::ptr::copy(DATA.as_ptr(), v.stable_mut_ptr(), 10);
        v.set_init(10);
    }

    assert_eq!(&v[..], &DATA[..10]);
}

#[test]
fn test_slice() {
    let v = &b""[..];

    assert_eq!(v.as_ptr(), v.stable_ptr());
    assert_eq!(v.bytes_init(), 0);
    assert_eq!(v.bytes_total(), 0);

    let v = &b"hello"[..];

    assert_eq!(v.as_ptr(), v.stable_ptr());
    assert_eq!(v.bytes_init(), 5);
    assert_eq!(v.bytes_total(), 5);
}

const DATA: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789!?";

macro_rules! test_slice {
    (
        $( $name:ident => $buf:expr; )*
    ) => {
        $(
            mod $name {
                use super::*;

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
                fn test_subslice_read() {
                    let buf = $buf;

                    let buf = test_subslice_read_case(buf.slice(..), DATA, ..);
                    let buf = test_subslice_read_case(buf.slice(..), DATA, 10..);
                    let buf = test_subslice_read_case(buf.slice(..), DATA, 5..15);
                    let buf = test_subslice_read_case(buf.slice(..), DATA, ..15);

                    let buf = test_subslice_read_case(buf.slice(5..), &DATA[5..], ..);
                    let buf = test_subslice_read_case(buf.slice(5..), &DATA[5..], 5..);
                    let buf = test_subslice_read_case(buf.slice(5..), &DATA[5..], 5..15);
                    let buf = test_subslice_read_case(buf.slice(5..), &DATA[5..], ..10);

                    let buf = test_subslice_read_case(buf.slice(5..25), &DATA[5..25], ..);
                    let buf = test_subslice_read_case(buf.slice(5..25), &DATA[5..25], 5..);
                    let buf = test_subslice_read_case(buf.slice(5..25), &DATA[5..25], 5..15);
                    let buf = test_subslice_read_case(buf.slice(5..25), &DATA[5..25], ..10);

                    let buf = test_subslice_read_case(buf.slice(..25), &DATA[..25], ..);
                    let buf = test_subslice_read_case(buf.slice(..25), &DATA[..25], 5..);
                    let buf = test_subslice_read_case(buf.slice(..25), &DATA[..25], 5..15);
                    let ___ = test_subslice_read_case(buf.slice(..25), &DATA[..25], ..10);
                }
            }
        )*
    };
}

fn test_subslice_read_case<B, R>(slice: Slice<B>, expected: &[u8], range: R) -> B
where
    B: IoBuf,
    R: RangeBounds<usize> + SliceIndex<[u8], Output = [u8]> + Clone,
{
    use std::ops::{Bound, Index};

    let buf_ptr = slice.get_ref().stable_ptr();
    let buf_total = slice.get_ref().bytes_total();
    let buf_init = slice.get_ref().bytes_init();

    let begin = slice.begin();
    let end = slice.end();
    let subslice = slice.slice(range.clone());
    let data = expected.index(range.clone());
    match range.start_bound() {
        Bound::Included(&n) => {
            assert_eq!(subslice.begin(), begin + n);
        }
        Bound::Excluded(&n) => {
            assert_eq!(subslice.begin(), begin + n + 1);
        }
        Bound::Unbounded => {
            assert_eq!(subslice.begin(), begin);
        }
    }
    match range.end_bound() {
        Bound::Included(&n) => {
            assert_eq!(subslice.end(), begin + n + 1);
        }
        Bound::Excluded(&n) => {
            assert_eq!(subslice.end(), begin + n);
        }
        Bound::Unbounded => {
            assert_eq!(subslice.end(), end);
        }
    }
    assert_eq!(&subslice[..], data);

    let buf = subslice.into_inner();
    assert_eq!(buf.stable_ptr(), buf_ptr);
    assert_eq!(buf.bytes_init(), buf_init);
    assert_eq!(buf.bytes_total(), buf_total);
    buf
}

test_slice! {
    vec => Vec::from(DATA);
    slice => DATA;
}

#[test]
fn can_deref_slice_into_uninit_buf() {
    let buf = Vec::with_capacity(10).slice(..);
    let _ = buf.stable_ptr();
    assert_eq!(buf.bytes_init(), 0);
    assert_eq!(buf.bytes_total(), 10);
    assert!(buf[..].is_empty());

    let mut v = Vec::with_capacity(10);
    v.push(42);
    let mut buf = v.slice(..);
    let _ = buf.stable_mut_ptr();
    assert_eq!(buf.bytes_init(), 1);
    assert_eq!(buf.bytes_total(), 10);
    assert_eq!(mem::replace(&mut buf[0], 0), 42);
    buf.copy_from_slice(&[43]);
    assert_eq!(&buf[..], &[43]);
}

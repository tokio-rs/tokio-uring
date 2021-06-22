mod io_buf;
pub use io_buf::IoBuf;

mod io_buf_mut;
pub use io_buf_mut::IoBufMut;

use std::ops;

/*
fn range(range: impl ops::RangeBounds<usize>, max: usize) -> (usize, usize) {
    use core::ops::Bound;

    let begin = match range.start_bound() {
        Bound::Included(&n) => n,
        Bound::Excluded(&n) => n + 1,
        Bound::Unbounded => 0,
    };

    assert!(begin < max);

    let end = match range.end_bound() {
        Bound::Included(&n) => n.checked_add(1).expect("out of range"),
        Bound::Excluded(&n) => n,
        Bound::Unbounded => max,
    };

    assert!(end <= max);

    (begin, end)
}
*/

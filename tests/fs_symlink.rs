#[path = "../src/future.rs"]
#[allow(warnings)]
mod future;

use std::io::Write;
use tokio_test::assert_ok;
use tokio_uring::fs;

use tempfile::tempdir;
use tempfile::NamedTempFile;

const TEST_PAYLOAD: &[u8] = b"I am data in the source file";

#[test]
fn test_create_symlink() {
    tokio_uring::start(async {
        let mut src_file = NamedTempFile::new().unwrap();
        src_file.write_all(TEST_PAYLOAD).unwrap();

        let dst_enclosing_dir = tempdir().unwrap();

        assert_ok!(fs::symlink(src_file.path(), dst_enclosing_dir.path().join("abc")).await);

        let content = std::fs::read(dst_enclosing_dir.path().join("abc")).unwrap();

        assert_eq!(content, TEST_PAYLOAD);
    });
}

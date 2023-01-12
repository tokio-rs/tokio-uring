#[path = "../src/future.rs"]
#[allow(warnings)]
mod future;

use tokio_test::assert_ok;
use tokio_uring::fs;

use tempfile::tempdir;

#[tokio_uring::test]
async fn basic_create_dir() {
    let base_dir = tempdir().unwrap();
    let new_dir = base_dir.path().join("foo");
    let new_dir_2 = new_dir.clone();

    assert_ok!(fs::create_dir(new_dir).await);

    assert!(new_dir_2.is_dir());
}

#[tokio_uring::test]
async fn basic_remove_dir() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    tokio_uring::fs::remove_dir(temp_dir.path()).await.unwrap();
    assert!(std::fs::metadata(temp_dir.path()).is_err());
}

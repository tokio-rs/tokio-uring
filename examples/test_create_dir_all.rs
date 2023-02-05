use std::io;
use std::path::Path;
use tokio_uring::fs;

fn tests() -> std::slice::Iter<'static, Expected<'static>> {
    [
        //
        // A number of Fail cases because of permissions (assuming not running as root).
        //
        Expected::Fail(Op::create_dir("/no-good")),
        Expected::Fail(Op::create_dir("/no-good/lots/more")),
        Expected::Fail(Op::create_dir_all("/no-good")),
        Expected::Fail(Op::create_dir_all("/no-good/lots/more")),
        Expected::Fail(Op::DirBuilder("/no-good")),
        Expected::Fail(Op::DirBuilder2("/no-good/lots/more", false, 0o777)),
        Expected::Fail(Op::DirBuilder2("/no-good/lots/more", true, 0o777)),
        //
        // A sequence of steps where assumption is /tmp exists and /tmp/test-good does not.
        //
        Expected::Pass(Op::create_dir("/tmp/test-good")),
        Expected::Pass(Op::statx("/tmp/test-good")),
        Expected::Pass(Op::StatxBuilder("/tmp/test-good")),
        Expected::Pass(Op::StatxBuilder2("/tmp", "test-good")),
        Expected::Pass(Op::StatxBuilder2("/tmp", "./test-good")),
        Expected::Pass(Op::StatxBuilder2("/tmp/", "./test-good")),
        Expected::Pass(Op::StatxBuilder2("/etc/", "/tmp/test-good")),
        Expected::Pass(Op::is_dir("/tmp/test-good")),
        Expected::Fail(Op::is_regfile("/tmp/test-good")),
        Expected::Pass(Op::create_dir("/tmp/test-good/x1")),
        Expected::Fail(Op::create_dir("/tmp/test-good/x1")),
        Expected::Pass(Op::remove_dir("/tmp/test-good/x1")),
        Expected::Fail(Op::remove_dir("/tmp/test-good/x1")),
        Expected::Pass(Op::remove_dir("/tmp/test-good")),
        Expected::Pass(Op::create_dir_all("/tmp/test-good/lots/lots/more")),
        Expected::Pass(Op::create_dir_all("/tmp/test-good/lots/lots/more")),
        Expected::Pass(Op::remove_dir("/tmp/test-good/lots/lots/more")),
        Expected::Pass(Op::remove_dir("/tmp/test-good/lots/lots")),
        Expected::Pass(Op::remove_dir("/tmp/test-good/lots")),
        Expected::Pass(Op::remove_dir("/tmp/test-good")),
        Expected::Fail(Op::statx("/tmp/test-good")),
        Expected::Fail(Op::StatxBuilder("/tmp/test-good")),
        //
        // A sequence that tests when mode is passed as 0, the directory can't be written to.
        //
        Expected::Pass(Op::DirBuilder2("/tmp/test-good", true, 0)),
        Expected::Pass(Op::matches_mode("/tmp/test-good", 0)),
        Expected::Fail(Op::create_dir("/tmp/test-good/x1")),
        Expected::Pass(Op::remove_dir("/tmp/test-good")),
        //
        // A sequence that tests creation of a user rwx only directory
        //
        Expected::Pass(Op::DirBuilder2("/tmp/test-good", true, 0o700)),
        Expected::Pass(Op::matches_mode("/tmp/test-good", 0o700)),
        Expected::Pass(Op::create_dir("/tmp/test-good/x1")),
        Expected::Pass(Op::remove_dir("/tmp/test-good/x1")),
        Expected::Pass(Op::remove_dir("/tmp/test-good")),
        //
        // Same sequence but with recursive = false
        //
        Expected::Pass(Op::DirBuilder2("/tmp/test-good", false, 0)),
        Expected::Fail(Op::create_dir("/tmp/test-good/x1")),
        Expected::Pass(Op::remove_dir("/tmp/test-good")),
        //
        // Some file operations
        //
        Expected::Pass(Op::touch_file("/tmp/test-good-file")),
        Expected::Pass(Op::is_regfile("/tmp/test-good-file")),
        Expected::Fail(Op::is_dir("/tmp/test-good-file")),
        Expected::Pass(Op::remove_file("/tmp/test-good-file")),
        Expected::Fail(Op::is_regfile("/tmp/test-good-file")),
        Expected::Fail(Op::is_dir("/tmp/test-good-file")),
    ]
    .iter()
}

type OpPath<'a> = &'a str;

#[allow(non_camel_case_types)]
#[allow(dead_code)]
#[derive(Debug)]
enum Op<'a> {
    statx(OpPath<'a>),
    StatxBuilder(OpPath<'a>),
    StatxBuilder2(OpPath<'a>, OpPath<'a>),
    matches_mode(OpPath<'a>, u16),
    is_regfile(OpPath<'a>),
    is_dir(OpPath<'a>),
    touch_file(OpPath<'a>),
    create_dir(OpPath<'a>),
    create_dir_all(OpPath<'a>),
    DirBuilder(OpPath<'a>),
    DirBuilder2(OpPath<'a>, bool, u32),
    remove_file(OpPath<'a>),
    remove_dir(OpPath<'a>),
}

#[derive(Debug)]
enum Expected<'a> {
    Pass(Op<'a>),
    Fail(Op<'a>),
}

async fn main1() -> io::Result<()> {
    let (mut as_expected, mut unexpected) = (0, 0);

    for test in tests() {
        let (expect_to_pass, op) = match test {
            Expected::Pass(op) => (true, op),
            Expected::Fail(op) => (false, op),
        };
        let res = match op {
            Op::statx(path) => statx(path).await,
            Op::StatxBuilder(path) => statx_builder(path).await,
            Op::StatxBuilder2(path, rel_path) => statx_builder2(path, rel_path).await,
            Op::matches_mode(path, mode) => matches_mode(path, *mode).await,
            Op::is_regfile(path) => is_regfile(path).await,
            Op::is_dir(path) => is_dir(path).await,
            Op::touch_file(path) => touch_file(path).await,
            Op::create_dir(path) => fs::create_dir(path).await,
            Op::create_dir_all(path) => fs::create_dir_all(path).await,
            Op::DirBuilder(path) => fs::DirBuilder::new().create(path).await,
            Op::DirBuilder2(path, recursive, mode) => {
                fs::DirBuilder::new()
                    .recursive(*recursive)
                    .mode(*mode)
                    .create(path)
                    .await
            }
            Op::remove_file(path) => fs::remove_file(path).await,
            Op::remove_dir(path) => fs::remove_dir(path).await,
        };

        let verbose = true;

        match res {
            Ok(_) => {
                if expect_to_pass {
                    as_expected += 1;
                    if verbose {
                        println!("Success: {op:?} passed.");
                    }
                } else {
                    unexpected += 1;
                    println!("Failure: {op:?} expected to fail but passed.");
                }
            }
            Err(e) => {
                if expect_to_pass {
                    unexpected += 1;
                    println!("Failure: {op:?} expected to pass but failed with error \"{e}\".");
                } else {
                    as_expected += 1;
                    if verbose {
                        println!("Success: {op:?} expected to fail and did with error \"{e}\".");
                    }
                }
            }
        }
    }

    println!("{as_expected} as_expected, {unexpected} unexpected");

    if unexpected == 0 {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("{unexpected} unexpected result(s)"),
        ))
    }
}

async fn statx<P: AsRef<Path>>(path: P) -> io::Result<()> {
    let _statx = tokio_uring::fs::statx(path).await?;
    Ok(())
}

async fn statx_builder<P: AsRef<Path>>(path: P) -> io::Result<()> {
    let _statx = tokio_uring::fs::StatxBuilder::new()
        .statx_path(path)
        .await?;
    Ok(())
}

async fn statx_builder2<P: AsRef<Path>>(dir_path: P, rel_path: P) -> io::Result<()> {
    // This shows the power of combining an open file, presumably a directory, and the relative
    // path to have the statx operation return the meta data for the child of the opened directory
    // descriptor.
    let f = tokio_uring::fs::File::open(dir_path).await.unwrap();

    // Fetch file metadata
    let res = f.statx_builder().statx_path(rel_path).await;

    // Close the file
    f.close().await.unwrap();

    res.map(|_| ())
}

async fn matches_mode<P: AsRef<Path>>(path: P, want_mode: u16) -> io::Result<()> {
    let statx = tokio_uring::fs::StatxBuilder::new()
        .mask(libc::STATX_MODE)
        .statx_path(path)
        .await?;
    let got_mode = statx.stx_mode & 0o7777;
    if want_mode == got_mode {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("want mode {want_mode:#o}, got mode {got_mode:#o}"),
        ))
    }
}

async fn touch_file<P: AsRef<Path>>(path: P) -> io::Result<()> {
    let file = tokio_uring::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .await?;

    file.close().await
}

async fn is_regfile<P: AsRef<Path>>(path: P) -> io::Result<()> {
    let (_is_dir, is_regfile) = tokio_uring::fs::is_dir_regfile(path).await;

    if is_regfile {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "not regular file",
        ))
    }
}

async fn is_dir<P: AsRef<Path>>(path: P) -> io::Result<()> {
    let (is_dir, _is_regfile) = tokio_uring::fs::is_dir_regfile(path).await;

    if is_dir {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "not directory",
        ))
    }
}

fn main() {
    tokio_uring::start(async {
        if let Err(e) = main1().await {
            println!("error: {}", e);
        }
    });
}

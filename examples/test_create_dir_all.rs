use std::io;
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
        //
        // A sequence that tests when mode is passed as 0, the directory can't be written to.
        //
        Expected::Pass(Op::DirBuilder2("/tmp/test-good", true, 0)),
        Expected::Fail(Op::create_dir("/tmp/test-good/x1")),
        Expected::Pass(Op::remove_dir("/tmp/test-good")),
        //
        // Same sequence but with recursive = false
        //
        Expected::Pass(Op::DirBuilder2("/tmp/test-good", false, 0)),
        Expected::Fail(Op::create_dir("/tmp/test-good/x1")),
        Expected::Pass(Op::remove_dir("/tmp/test-good")),
    ]
    .iter()
}

type OpPath<'a> = &'a str;

#[allow(non_camel_case_types)]
#[allow(dead_code)]
#[derive(Debug)]
enum Op<'a> {
    FileExists(OpPath<'a>),
    HasMode(OpPath<'a>, u32),
    DirExists(OpPath<'a>),
    TouchFile(OpPath<'a>),
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
            Op::FileExists(_path) => {
                unreachable!("FileExists unimplemented");
            }
            Op::HasMode(_path, _mode) => {
                unreachable!("HasMode unimplemented");
            }
            Op::DirExists(_path) => {
                unreachable!("DirExists unimplemented");
            }
            Op::TouchFile(_path) => {
                unreachable!("TouchFile unimplemented");
            }
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

        let verbose = false;

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

fn main() {
    tokio_uring::start(async {
        if let Err(e) = main1().await {
            println!("error: {}", e);
        }
    });
}

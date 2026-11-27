//! The sketch's calls, run on the host. Fixtures are made under the sketch's
//! own target directory so nothing outside it is touched.

use std::ffi::CString;

use no_std_file_reading_on_unix::{Fail, list, read_into, shell, var};

fn here(name: &str) -> std::path::PathBuf {
    let d = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("no-std-file-reading");
    std::fs::create_dir_all(&d).unwrap();
    d.join(name)
}

fn c(p: &std::path::Path) -> CString {
    CString::new(p.as_os_str().as_encoded_bytes()).unwrap()
}

#[test]
fn a_file_is_read_whole_into_a_buffer_that_fits() {
    let p = here("fits.toml");
    std::fs::write(&p, b"name = \"x\"\n").unwrap();
    let mut buf = [0u8; 64];
    assert_eq!(read_into(&c(&p), &mut buf).unwrap(), b"name = \"x\"\n");
}

#[test]
fn a_file_bigger_than_the_buffer_says_the_size_it_needs() {
    let p = here("big.toml");
    std::fs::write(&p, [b'a'; 100]).unwrap();
    let mut buf = [0u8; 10];
    assert_eq!(
        read_into(&c(&p), &mut buf),
        Err(Fail::TooBig {
            need: 100,
        })
    );
    let mut fits = [0u8; 100];
    assert_eq!(read_into(&c(&p), &mut fits).unwrap().len(), 100);
}

#[test]
fn a_missing_file_is_refused_with_enoent() {
    let mut buf = [0u8; 8];
    assert_eq!(
        read_into(&c(&here("absent")), &mut buf),
        Err(Fail::Open(libc::ENOENT))
    );
}

#[test]
fn a_directory_lists_its_entries() {
    let d = here("dir");
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("app.toml"), b"").unwrap();
    std::fs::write(d.join("app.local.toml"), b"").unwrap();
    let mut seen = Vec::new();
    list(&c(&d), |n| seen.push(n.to_str().unwrap().to_owned())).unwrap();
    seen.sort();
    assert_eq!(seen, ["app.local.toml", "app.toml"]);
}

#[test]
fn a_variable_is_read_and_an_unset_one_is_none() {
    assert!(var(c"PATH").is_some());
    assert_eq!(var(c"NO_SUCH_VARIABLE_IN_THIS_SKETCH"), None);
}

#[test]
fn a_shell_command_runs_in_the_directory_and_answers_its_exit() {
    let d = here("shell-dir");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("marker"), b"").unwrap();
    assert_eq!(shell(&c(&d), c"test -f marker").unwrap(), 0);
    assert_eq!(shell(&c(&d), c"test -f not-here").unwrap(), 1);
    assert_eq!(shell(&c(&d), c"exit 2").unwrap(), 2);
}

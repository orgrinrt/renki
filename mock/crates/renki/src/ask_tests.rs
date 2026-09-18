use std::cell::Cell;
use std::path::Path;

use super::*;

fn sh(script: &str) -> Command {
    let mut c = Command::new("sh");
    c.args(["-c", script]);
    c
}

#[test]
fn a_child_past_its_deadline_is_killed_and_reported() {
    let started = Instant::now();
    let why = run_within(sh("sleep 30"), Duration::from_millis(200), "sleeper").unwrap_err();
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    assert!(why.contains("sleeper did not answer within"), "{why}");
}

#[test]
fn a_child_inside_its_deadline_hands_back_both_streams_and_its_status() {
    let out = run_within(
        sh("echo out; echo err >&2; exit 3"),
        Duration::from_secs(5),
        "x",
    )
    .unwrap();
    assert_eq!(out.stdout, b"out\n");
    assert_eq!(out.stderr, b"err\n");
    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn a_child_writing_more_than_a_pipe_holds_is_not_stalled() {
    let out = run_within(sh("head -c 1000000 /dev/zero"), Duration::from_secs(5), "x").unwrap();
    assert!(out.status.success());
    assert_eq!(out.stdout.len(), 1_000_000);
}

/// Set in a child of the test binary, which then runs the one test it names.
const CHILD: &str = "RENKI_ASK_CHILD";

/// Run the ignored test `name` again as a child of the test binary, in `dir`
/// with `env` set, every ssh setting and the global and system configuration
/// cleared, and stdin a pipe held open until it exits. What it printed.
fn child_test(name: &str, dir: &Path, env: &[(&str, &str)]) -> String {
    let mut child = Command::new(std::env::current_exe().unwrap());
    child
        .args([
            "--exact",
            &format!("ask::tests::{name}"),
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .current_dir(dir)
        .env(CHILD, "1")
        .env_remove("GIT_SSH_COMMAND")
        .env_remove("GIT_SSH")
        .env_remove("GIT_DIR")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        child.env(k, v);
    }
    let mut running = child.spawn().unwrap();
    // Held, and so open, until the child is done.
    let held = running.stdin.take();
    let out = running.wait_with_output().unwrap();
    drop(held);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The value the child printed as `name=`.
fn printed(stdout: &str, name: &str) -> String {
    stdout
        .lines()
        .find_map(|l| l.strip_prefix(&format!("{name}=")))
        .unwrap_or_else(|| panic!("the child printed no {name}: {stdout}"))
        .to_string()
}

#[test]
fn a_child_reading_stdin_reads_end_of_file_where_stdin_never_closes() {
    let dir = tempfile::tempdir().unwrap();
    let out = child_test("stdin_reports_itself", dir.path(), &[]);
    assert_eq!(printed(&out, "read"), "\"done\\n\"");
}

#[test]
#[ignore = "run as a child by child_test, whose stdin is a pipe it holds open"]
fn stdin_reports_itself() {
    assert!(
        std::env::var_os(CHILD).is_some(),
        "run only as a child, by `child_test`"
    );
    // A child inheriting this process's stdin would wait on the open pipe
    // until the deadline and come back as an error.
    let out = run_within(sh("cat; echo done"), Duration::from_secs(2), "reader");
    println!();
    println!(
        "read={:?}",
        out.map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_else(|e| e)
    );
}

#[test]
fn a_child_that_leaves_its_output_held_open_is_answered_at_its_exit() {
    // The background sleeper holds stdout for four seconds after the child
    // exits. The answer is what the child wrote, as soon as it exited.
    let started = Instant::now();
    let out = run_within(sh("sleep 4 & echo hi"), Duration::from_secs(5), "leaver").unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(out.stdout, b"hi\n");
}

#[test]
fn a_child_that_cannot_start_is_an_error_not_a_hang() {
    let why = run_within(
        Command::new("/nonexistent/binary"),
        Duration::from_secs(5),
        "ghost",
    )
    .unwrap_err();
    assert!(why.contains("could not run ghost"), "{why}");
}

#[test]
fn a_scratch_file_is_gone_once_dropped_and_two_never_share_a_name() {
    let a = Scratch::new("x").unwrap();
    let b = Scratch::new("x").unwrap();
    assert_ne!(a.path, b.path);
    let (pa, pb) = (a.path.clone(), b.path.clone());
    assert!(pa.exists() && pb.exists());
    drop(a);
    assert!(!pa.exists(), "{pa:?} outlived its scratch");
    drop(b);
    assert!(!pb.exists(), "{pb:?} outlived its scratch");
}

#[test]
fn a_scratch_file_reads_back_from_its_start_whatever_was_written() {
    let mut s = Scratch::new("x").unwrap();
    let mut child = sh("printf abc")
        .stdout(s.for_child("x").unwrap())
        .spawn()
        .unwrap();
    child.wait().unwrap();
    assert_eq!(s.read_back("x").unwrap(), b"abc");
}

/// Git's three ssh settings as a test states them, counting how often the
/// configuration was asked and taking `core_takes` to answer it.
struct Sources {
    ssh_command: Option<&'static str>,
    ssh:         Option<&'static str>,
    core:        Option<&'static str>,
    core_takes:  Duration,
    core_asked:  Cell<u32>,
}

impl Sources {
    fn of(
        ssh_command: Option<&'static str>,
        ssh: Option<&'static str>,
        core: Option<&'static str>,
    ) -> Self {
        Self {
            ssh_command,
            ssh,
            core,
            core_takes: Duration::ZERO,
            core_asked: Cell::new(0),
        }
    }

    fn silent() -> Self {
        Self::of(None, None, None)
    }
}

impl SshSources for Sources {
    fn ssh_command(&self) -> Option<OsString> {
        self.ssh_command.map(Into::into)
    }

    fn ssh(&self) -> Option<OsString> {
        self.ssh.map(Into::into)
    }

    fn core_ssh_command(&self, _: Duration) -> Option<OsString> {
        self.core_asked.set(self.core_asked.get() + 1);
        std::thread::sleep(self.core_takes);
        self.core.map(Into::into)
    }
}

fn env_of(git: &Command, key: &str) -> Option<OsString> {
    git.get_envs()
        .find(|(k, _)| *k == key)
        .and_then(|(_, v)| v.map(|v| v.to_os_string()))
}

#[test]
fn quiet_git_turns_the_prompt_and_a_helpers_window_off_whatever_ssh_is() {
    for sources in [Sources::silent(), Sources::of(Some("ssh -i key"), None, None)] {
        let mut git = quiet_git(&sources, ASK_DEADLINE);
        assert_eq!(git.get_program(), "git");
        assert_eq!(env_of(&git, "GIT_TERMINAL_PROMPT"), Some("0".into()));
        // Ahead of the subcommand, where git reads a global option.
        git.arg("ls-remote");
        let args: Vec<_> = git.get_args().collect();
        assert_eq!(args, ["-c", "credential.interactive=never", "ls-remote"]);
    }
}

#[test]
fn the_git_that_runs_reads_credential_interactive_as_never() {
    // What git itself makes of the argument, with no configuration of anybody's
    // underneath: the setting a helper is handed is the one this reads back.
    let dir = tempfile::tempdir().unwrap();
    let mut git = quiet_git(&Sources::silent(), ASK_DEADLINE);
    git.current_dir(dir.path())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args(["config", "--get", "credential.interactive"]);
    let out = run_within(git, ASK_DEADLINE, "config").unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "never");
}

#[test]
fn ssh_is_put_in_batch_mode_only_when_none_of_the_three_says_how_it_runs() {
    let set = Some("ssh -i key");
    for ssh_command in [None, set] {
        for ssh in [None, set] {
            for core in [None, set] {
                let got = env_of(
                    &quiet_git(&Sources::of(ssh_command, ssh, core), ASK_DEADLINE),
                    "GIT_SSH_COMMAND",
                );
                let silent = ssh_command.is_none() && ssh.is_none() && core.is_none();
                let want = silent.then(|| "ssh -o BatchMode=yes".into());
                assert_eq!(
                    got, want,
                    "GIT_SSH_COMMAND={ssh_command:?} GIT_SSH={ssh:?} core={core:?}"
                );
            }
        }
    }
}

#[test]
fn the_configuration_is_asked_only_when_the_environment_says_nothing() {
    let asked = |sources: Sources| {
        quiet_git(&sources, ASK_DEADLINE);
        sources.core_asked.get()
    };
    assert_eq!(asked(Sources::silent()), 1);
    assert_eq!(asked(Sources::of(None, None, Some("ssh -i key"))), 1);
    assert_eq!(asked(Sources::of(Some("ssh -i key"), None, None)), 0);
    assert_eq!(asked(Sources::of(None, Some("ssh"), None)), 0);
}

#[test]
fn one_deadline_covers_the_configuration_and_the_listing_both() {
    // The configuration takes the whole deadline, so the listing is left none
    // of it and is given up on at once. Given the full deadline again instead,
    // git would run to its own answer: a failure about the path.
    let deadline = Duration::from_millis(600);
    let sources = Sources {
        core_takes: deadline,
        ..Sources::silent()
    };
    let started = Instant::now();
    let why =
        crate::pin::ls_remote_head_from(&sources, "file:///nonexistent/pack.git", "dev", deadline)
            .unwrap_err();
    assert!(why.contains("did not answer within 0 seconds"), "{why}");
    assert!(
        started.elapsed() < deadline + Duration::from_millis(400),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn a_listing_pointed_at_an_ssh_that_hangs_comes_back_within_the_deadline() {
    let mut git = quiet_git(&Sources::silent(), ASK_DEADLINE);
    git.args(["ls-remote", "ssh://git@example.invalid/nothing", "refs/heads/dev"])
        .env("GIT_SSH_COMMAND", "sh -c 'sleep 30' --");
    let started = Instant::now();
    let why = run_within(git, Duration::from_millis(500), "listing").unwrap_err();
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    assert!(why.contains("listing did not answer within"), "{why}");
}

#[test]
fn the_configuration_is_read_as_git_would_read_it_here() {
    let git = core_ssh_command_query();
    assert_eq!(git.get_program(), "git");
    let args: Vec<_> = git.get_args().collect();
    assert_eq!(args, ["config", "--get", "core.sshCommand"]);
}

#[test]
fn a_configured_value_is_what_the_repository_sets_and_nothing_otherwise() {
    let dir = tempfile::tempdir().unwrap();
    let isolated = |mut c: Command| {
        c.current_dir(dir.path())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1");
        c
    };
    let git = |args: &[&str]| {
        let mut c = Command::new("git");
        c.args(args);
        isolated(c).status().unwrap().success()
    };
    assert!(git(&["init", "-q"]));
    let read = || configured_value(isolated(core_ssh_command_query()), ASK_DEADLINE);
    assert_eq!(read(), None);
    assert!(git(&["config", "core.sshCommand", "ssh -i key"]));
    assert_eq!(read(), Some("ssh -i key".into()));
    assert!(git(&["config", "core.sshCommand", ""]));
    assert_eq!(read(), None);
}

#[test]
fn a_configuration_too_slow_or_failing_reads_as_nothing_set() {
    let started = Instant::now();
    assert_eq!(
        configured_value(sh("sleep 30; echo late"), Duration::from_millis(300)),
        None
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(
        configured_value(sh("echo 'ssh -i key'"), ASK_DEADLINE),
        Some("ssh -i key".into())
    );
    assert_eq!(
        configured_value(sh("echo 'ssh -i key'; exit 1"), ASK_DEADLINE),
        None
    );
}

// --- this process, read from a child of the test binary --------------------------

/// `git` in `dir` with nobody's configuration but the repository's own.
fn git_in(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

fn sources_in(dir: &Path, env: &[(&str, &str)]) -> [String; 3] {
    let out = child_test("this_process_reports_itself", dir, env);
    ["ssh_command", "ssh", "core"].map(|name| printed(&out, name))
}

fn batch_in(dir: &Path, env: &[(&str, &str)]) -> String {
    printed(
        &child_test("this_process_reports_itself", dir, env),
        "batch",
    )
}

#[test]
#[ignore = "run as a child by child_test, which reads what it prints"]
fn this_process_reports_itself() {
    assert!(
        std::env::var_os(CHILD).is_some(),
        "run only as a child, by `child_test`"
    );
    let p = ThisProcess;
    // The harness prints the test's name with no newline before its output.
    println!();
    println!("ssh_command={:?}", p.ssh_command());
    println!("ssh={:?}", p.ssh());
    println!("core={:?}", p.core_ssh_command(ASK_DEADLINE));
    println!(
        "batch={:?}",
        env_of(&quiet_git(&p, ASK_DEADLINE), "GIT_SSH_COMMAND")
    );
}

#[test]
fn this_process_reads_both_variables_and_the_clone_it_runs_in() {
    let dir = tempfile::tempdir().unwrap();
    git_in(dir.path(), &["init", "-q"]);
    assert_eq!(sources_in(dir.path(), &[]), ["None", "None", "None"]);
    assert_eq!(
        sources_in(dir.path(), &[
            ("GIT_SSH_COMMAND", "ssh -i one"),
            ("GIT_SSH", "/bin/two")
        ]),
        [r#"Some("ssh -i one")"#, r#"Some("/bin/two")"#, "None"]
    );
    git_in(dir.path(), &["config", "core.sshCommand", "ssh -i three"]);
    assert_eq!(sources_in(dir.path(), &[]), [
        "None",
        "None",
        r#"Some("ssh -i three")"#
    ]);
}

#[test]
fn this_process_puts_ssh_in_batch_mode_only_in_a_clone_that_says_nothing() {
    let dir = tempfile::tempdir().unwrap();
    git_in(dir.path(), &["init", "-q"]);
    assert_eq!(batch_in(dir.path(), &[]), r#"Some("ssh -o BatchMode=yes")"#);
    assert_eq!(batch_in(dir.path(), &[("GIT_SSH", "/bin/two")]), "None");
    // Nothing set over it, so git reads the one the environment carries.
    assert_eq!(
        batch_in(dir.path(), &[("GIT_SSH_COMMAND", "ssh -i one")]),
        "None"
    );
    git_in(dir.path(), &["config", "core.sshCommand", "ssh -i three"]);
    assert_eq!(batch_in(dir.path(), &[]), "None");
}

// --- the wrapper both callers reach ----------------------------------------------

/// A repository in `dir` with one commit on `dev`, and that commit.
fn repository_with_dev(dir: &Path) -> String {
    git_in(dir, &["init", "-q", "-b", "dev"]);
    git_in(dir, &[
        "-c",
        "user.name=t",
        "-c",
        "user.email=t@t",
        "commit",
        "-q",
        "--allow-empty",
        "-m",
        "x",
    ]);
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn the_launchers_own_ask_answers_a_branch_head_from_a_repository() {
    let dir = tempfile::tempdir().unwrap();
    let head = repository_with_dev(dir.path());
    let url = format!("file://{}", dir.path().display());
    assert_eq!(crate::pin::ls_remote_head(&url, "dev"), Ok(head));
}

#[test]
fn the_launchers_own_ask_says_a_missing_branch_is_not_there() {
    let dir = tempfile::tempdir().unwrap();
    repository_with_dev(dir.path());
    // A tag of the name asked for is not the branch, and is not answered as it.
    git_in(dir.path(), &["tag", "main"]);
    let url = format!("file://{}", dir.path().display());
    let why = crate::pin::ls_remote_head(&url, "main").unwrap_err();
    assert!(why.contains("branch 'main' not found"), "{why}");
}

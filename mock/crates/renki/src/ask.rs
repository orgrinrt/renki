//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Running a child that has to answer within a deadline, and the `git` that
//! asks a remote.
//!
//! `git ls-remote` is the one caller. `Command::output` waits for as long as the
//! child does, and on a network dropping packets git takes a long time to give
//! up, so the launcher stalled for as long as git took before the fallback that
//! exists for exactly that network could start.

use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// How long a remote is given to answer before it counts as unreachable.
pub(crate) const ASK_DEADLINE: Duration = Duration::from_secs(5);

/// Run `cmd` to completion, or kill it once `deadline` has passed.
///
/// stdin is closed, so a child that would ask for something reads end of file
/// instead. stdout and stderr go to two scratch files rather than pipes, so a
/// child writing more than a pipe holds never stalls on a parent that is only
/// waiting for it to exit, and nothing the child leaves running, an ssh control
/// master say, can hold the answer back: what is read is what the child had
/// written when it exited.
///
/// Only the child is killed at the deadline. What it started is left to finish
/// on its own, because the standard library has no way to signal a process
/// group.
pub(crate) fn run_within(
    mut cmd: Command,
    deadline: Duration,
    what: &str,
) -> Result<Output, String> {
    let started = Instant::now();
    let mut stdout = Scratch::new(what)?;
    let mut stderr = Scratch::new(what)?;
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(stdout.for_child(what)?)
        .stderr(stderr.for_child(what)?)
        .spawn()
        .map_err(|e| format!("could not run {what}: {e}"))?;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{what} did not answer within {} seconds",
                    deadline.as_secs_f32()
                ));
            },
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(format!("could not wait for {what}: {e}")),
        }
    };
    Ok(Output {
        status,
        stdout: stdout.read_back(what)?,
        stderr: stderr.read_back(what)?,
    })
}

/// A file one stream of a child is written to, removed when this is dropped,
/// on every path out of [`run_within`].
struct Scratch {
    path: PathBuf,
    file: File,
}

impl Scratch {
    fn new(what: &str) -> Result<Self, String> {
        let dir = std::env::temp_dir();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        // A name taken by another call in the same nanosecond is refused by
        // `create_new` rather than shared, and the next attempt gets another.
        for attempt in 0 .. 64 {
            let path = dir.join(format!(
                "renki-ask-{}-{stamp}-{attempt}",
                std::process::id()
            ));
            match OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(file) => {
                    return Ok(Self {
                        path,
                        file,
                    });
                },
                Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(format!("could not make a scratch file for {what}: {e}")),
            }
        }
        Err(format!(
            "could not make a scratch file for {what}: every name was taken"
        ))
    }

    fn for_child(&self, what: &str) -> Result<Stdio, String> {
        self.file
            .try_clone()
            .map(Stdio::from)
            .map_err(|e| format!("could not hand {what} its output file: {e}"))
    }

    fn read_back(&mut self, what: &str) -> Result<Vec<u8>, String> {
        let mut buf = Vec::new();
        self.file
            .seek(SeekFrom::Start(0))
            .and_then(|_| self.file.read_to_end(&mut buf))
            .map_err(|e| format!("could not read what {what} wrote: {e}"))?;
        Ok(buf)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// The three places git reads how to run ssh, highest ranked first.
/// `GIT_SSH_COMMAND` outranks both `GIT_SSH` and `core.sshCommand`, which is
/// why it is only ever set where none of the three says anything.
pub(crate) trait SshSources {
    fn ssh_command(&self) -> Option<OsString>;
    fn ssh(&self) -> Option<OsString>;
    fn core_ssh_command(&self, deadline: Duration) -> Option<OsString>;
}

/// This process's environment and the git configuration of the directory it
/// runs in, which is what the `git` it starts reads too.
pub(crate) struct ThisProcess;

impl SshSources for ThisProcess {
    fn ssh_command(&self) -> Option<OsString> {
        std::env::var_os("GIT_SSH_COMMAND")
    }

    fn ssh(&self) -> Option<OsString> {
        std::env::var_os("GIT_SSH")
    }

    fn core_ssh_command(&self, deadline: Duration) -> Option<OsString> {
        configured_value(core_ssh_command_query(), deadline).map(OsString::from)
    }
}

/// `git` with its own terminal prompt off and a credential helper told not to
/// open a window, which Git Credential Manager honours and a helper ignoring
/// `credential.interactive` does not. A stored credential is still used. ssh
/// runs in `BatchMode` only where none of `sources` says how ssh is to run, so
/// a key or agent somebody chose there is kept. A configured ssh may still ask
/// on the terminal: the caller's deadline gives up on the answer, and the
/// prompt can stay up after it, since only git is killed.
///
/// The sources are read in rank order and stop at the first that answers, so
/// the configuration is only asked when the environment says nothing.
pub(crate) fn quiet_git(sources: &impl SshSources, deadline: Duration) -> Command {
    let mut git = Command::new("git");
    // A global option, so it goes ahead of whatever the caller adds, and it
    // holds for this one call without writing any configuration.
    git.args(["-c", "credential.interactive=never"]);
    git.env("GIT_TERMINAL_PROMPT", "0");
    let unconfigured = sources.ssh_command().is_none()
        && sources.ssh().is_none()
        && sources.core_ssh_command(deadline).is_none();
    if unconfigured {
        git.env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes");
    }
    git
}

/// The query for `core.sshCommand` as git would read it here.
pub(crate) fn core_ssh_command_query() -> Command {
    let mut git = Command::new("git");
    git.args(["config", "--get", "core.sshCommand"]);
    git
}

/// What a `git config --get` query answers within `deadline`, where it
/// answers with a value. Unset, failed and too slow all read as nothing set.
pub(crate) fn configured_value(query: Command, deadline: Duration) -> Option<String> {
    let out = run_within(query, deadline, "git config --get").ok()?;
    let value = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !value.is_empty()).then_some(value)
}

#[cfg(test)]
#[path = "ask_tests.rs"]
mod tests;

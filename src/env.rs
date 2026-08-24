//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Dropping the repo-location `GIT_*` variables git exports into hooks.
//!
//! A launcher runs routinely as a grandchild of a git hook, and it spawns
//! processes whose working directory is not the hook's. With `GIT_DIR`
//! inherited and `GIT_WORK_TREE` unset, git treats that working directory as
//! the top of the work tree, so every index path resolves against the wrong
//! tree; with a *relative* `GIT_DIR=.git` the same inheritance makes those
//! invocations fail outright. Found live in mockspace: a worktree commit
//! reported all 84 doc templates as untracked.
//!
//! Dropping `GIT_INDEX_FILE` means a `git commit -a` temporary index is not
//! consulted. For detection that changes nothing, since a scan covers staged,
//! unstaged and untracked state alike. For content it can: a file staged and
//! then modified again reads as its index blob, while a `commit -a` in flight
//! would commit the worktree blob.
//!
//! This also strips a `GIT_DIR`/`GIT_WORK_TREE` pair exported on purpose (the
//! bare dotfiles-repo pattern), which falls back to ordinary repo discovery
//! from the working directory.

/// The variables git exports into a hook to say where the repo is.
///
/// Public so the removal set is testable without touching the environment.
pub const GIT_REPO_ENV: &[&str] = &[
    "GIT_DIR",
    "GIT_COMMON_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_PREFIX",
];

/// Drop them, so every child `git` rediscovers the repo from its own working
/// directory.
///
/// # Safety
///
/// Call first thing in the process entry, before any other thread exists. The
/// environment is process-global and unsynchronised; mutating it while another
/// thread reads it, including indirectly through `Command::spawn` or any libc
/// function that walks `environ`, is undefined behaviour.
pub unsafe fn sanitize_git_env() {
    for var in GIT_REPO_ENV {
        // SAFETY: the caller guarantees no other thread exists yet.
        unsafe { std::env::remove_var(var) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_set_is_the_repo_location_variables_and_not_the_identity_ones() {
        // pinned against accidental narrowing, which is the way this fails: a
        // variable dropped from the list goes on being inherited and nothing
        // reports it.
        for want in [
            "GIT_DIR",
            "GIT_COMMON_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_PREFIX",
        ] {
            assert!(GIT_REPO_ENV.contains(&want), "{want} is no longer scrubbed");
        }
        assert_eq!(GIT_REPO_ENV.len(), 7, "something was added without a test");
    }

    #[test]
    fn the_authoring_variables_are_left_alone() {
        // the control, and the reason this is a list rather than a `GIT_*`
        // sweep: a hook's author and committer identity is not a repo location
        // and scrubbing it would rewrite who a commit came from.
        for keep in [
            "GIT_AUTHOR_NAME",
            "GIT_AUTHOR_EMAIL",
            "GIT_COMMITTER_NAME",
            "GIT_COMMITTER_EMAIL",
        ] {
            assert!(!GIT_REPO_ENV.contains(&keep), "{keep} must survive");
        }
    }
}

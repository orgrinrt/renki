//! Extensions: many tools, resolved at run time, each materialised by a backend.
//!
//! [`Tool`](crate::Tool) is the launcher half. It knows one engine at compile
//! time, builds it with cargo and hands over. That covers a two-part tool and
//! nothing else.
//!
//! An extension is the other shape. A host reads a set of tool descriptors out
//! of its own configuration, so the tools are heterogeneous, their backends are
//! named by a string rather than known statically, and most of what the host
//! does with them (listing them, printing what they offer, telling somebody a
//! command does not exist) must not fetch anything at all.
//!
//! So the two halves of the api are split by whether they touch the disk.
//! [`Descriptor`] is a parsed `tool.toml` and nothing more. [`Located`] is what
//! it becomes once a backend has put it somewhere. Listing reads descriptors;
//! only running resolves.
//!
//! # The backend contract
//!
//! [`Backend`] is a trait of associated functions, and a [`Registered`] is its
//! const fn-pointer form. That is deliberate rather than incidental: the trait
//! is what an implementor writes and reads as, the vtable is what a host can
//! hold a heterogeneous `&'static [Registered]` of, and neither needs `dyn`,
//! `Box` or an allocation. It is the same shape [`Hooks`](crate::Hooks) already
//! uses for the tool-specific decisions of the launcher half.
//!
//! ```no_run
//! # use renki::extension::{Backend, Descriptor, Registered, Registry};
//! # struct Git;
//! # impl Backend for Git {
//! #     const NAME: &'static str = "git";
//! #     fn fingerprint() -> String { String::new() }
//! #     fn materialise(_: &Descriptor, _: &std::path::Path) -> Result<(), String> { Ok(()) }
//! # }
//! static BACKENDS: &[Registered] = &[Registered::of::<Git>()];
//! let registry = Registry::new(BACKENDS);
//! ```

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::hash::Fnv;

/// Where a tool's material comes from.
///
/// A `git` source is fetched into the shared cache once per revision and reused
/// by every workspace pinned to it, which is the whole reason the mechanism
/// exists: the same tool declared by twenty workspaces is one copy on disk. A
/// `path` source is already present and is never cached, because there is
/// nothing to fetch and copying it would put a stale duplicate between the
/// author and their own edits.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Source {
    /// A remote repository and the revision to take. The revision is a commit
    /// rather than a branch wherever it matters: a branch means the tool can
    /// change under a workspace without the workspace changing, and the whole
    /// point of a pin is that it cannot.
    Git {
        /// The repository to fetch from.
        url: String,
        /// The revision to take, a commit rather than a branch wherever it
        /// matters: a branch means the tool changes under a workspace without
        /// the workspace changing.
        rev: String,
    },
    /// A directory, relative to the workspace root.
    Path {
        /// The directory, relative to the workspace root.
        path: String,
    },
}

/// One command a tool offers.
///
/// The summary lives here rather than behind the tool's own `--help` so that a
/// host can print what a tool offers without materialising it, and so that a
/// hand-written command list cannot drift from the dispatch table: this **is**
/// the dispatch table.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandDef {
    /// What the host invokes it as.
    pub name:    String,
    /// One line. It is what a listing shows.
    pub summary: String,
    /// The executable, relative to the materialised tool's root.
    ///
    /// A path and never a shell string. A one-liner worth existing is worth a
    /// file with a shebang and a test; shell inside a toml value is unquotable,
    /// untestable, and invisible to every lint the repository has.
    pub run:     String,
}

/// A parsed `tool.toml`. No i/o has happened and none is needed to read it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Descriptor {
    /// What the host invokes the tool as. Matches the directory it was found
    /// in, where it was found in one.
    pub name:     String,
    /// One line, shown in listings.
    pub summary:  String,
    /// For finding a tool without knowing its name.
    #[serde(default)]
    pub tags:     Vec<String>,
    /// Which backend materialises it, matched against a [`Registry`].
    pub backend:  String,
    /// Where its material comes from.
    pub source:   Source,
    /// Every command it offers.
    #[serde(default)]
    pub commands: Vec<CommandDef>,
    /// Whether the host should also expose this tool as a top level
    /// subcommand of its own, rather than only under a `tool <name>` verb.
    #[serde(default)]
    pub promote:  bool,
}

impl Descriptor {
    /// Parse one descriptor.
    pub fn parse(text: &str) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct Outer {
            tool: Descriptor,
        }
        let d = toml::from_str::<Outer>(text)
            .map(|o| o.tool)
            .map_err(|e| format!("could not read the tool descriptor: {e}"))?;
        d.check()?;
        Ok(d)
    }

    /// Refuse a descriptor whose values a fetcher or a spawn would read as
    /// something other than data.
    ///
    /// A descriptor can arrive from a git ref, so its values are not the
    /// workspace author's word. Two classes, and both are checked here because
    /// both reach a process:
    ///
    /// A `rev` of `--upload-pack=...` is a command to git rather than a
    /// revision, and a leading dash anywhere is the shape of that whole class.
    /// The callers also put a `--` sentinel in the argv; this is the other
    /// half, because a sentinel does not help for a value that is legal in the
    /// position it lands in.
    ///
    /// A `run` of `/bin/sh`, or of `../../../sh`, is the same class pointed at
    /// [`command`] instead of at git. `Path::join` discards its left side when
    /// the right is absolute, so an unchecked `run` names any executable on the
    /// machine and the tool root the descriptor was materialised into is not
    /// consulted at all.
    ///
    /// **This is public and is called again by [`locate`] and [`command`].**
    /// The fields are public and `Deserialize` is derived, so a descriptor can
    /// reach either of those without passing through [`Descriptor::parse`], and
    /// a check that only runs at construction guards nothing about a value that
    /// was never constructed that way.
    pub fn check(&self) -> Result<(), String> {
        let bad = |what: &str, v: &str| {
            Err(format!(
                "the tool `{}` has a {what} of `{v}`, which is not one",
                self.name
            ))
        };
        match &self.source {
            Source::Git { url, rev } => {
                // Forty hex is a full object name and is what git can fetch by.
                // A short prefix is not: `git fetch --depth 1 origin <7-hex>`
                // fails with `couldn't find remote ref`, measured against git
                // 2.55, so accepting one only defers the error to the fetch.
                if !rev.chars().all(|c| c.is_ascii_hexdigit()) || rev.len() != 40 {
                    return bad("revision", rev);
                }
                let ok = ["https://", "ssh://", "git://", "git@"];
                let Some(rest) = ok.iter().find_map(|p| url.strip_prefix(p)) else {
                    return bad("url", url);
                };
                // A dash after the scheme is a host git reads as an ssh flag,
                // `ssh://-oProxyCommand=...`. Current git refuses those itself;
                // this does not rely on that, because the version in front of
                // the user is not this crate's to choose.
                if rest.starts_with('-') || rest.is_empty() {
                    return bad("url", url);
                }
            },
            Source::Path { path } => {
                if path.is_empty() || path.starts_with('-') {
                    return bad("path", path);
                }
                if !Self::stays_inside(path) {
                    return bad("path", path);
                }
            },
        }
        for c in &self.commands {
            if c.run.is_empty() || c.run.starts_with('-') || !Self::stays_inside(&c.run) {
                return Err(format!(
                    "the tool `{}` has a command `{}` that runs `{}`, which is not inside it",
                    self.name, c.name, c.run
                ));
            }
        }
        Ok(())
    }

    /// Whether a relative path can only ever resolve under the root it is
    /// joined to.
    ///
    /// Component-wise rather than by substring. `..` as a substring catches
    /// `a..b`, which is a legal name, and misses nothing a component scan does;
    /// and a root or prefix component is what makes `join` throw the root away.
    fn stays_inside(p: &str) -> bool {
        use std::path::Component;
        Path::new(p).components().all(|c| {
            matches!(
                c,
                Component::Normal(_) | Component::CurDir
            )
        })
    }

    /// Read a descriptor from a `tool.toml`.
    pub fn read(file: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(file)
            .map_err(|e| format!("could not read {}: {e}", file.display()))?;
        Self::parse(&text)
    }

    /// One command by name.
    pub fn command(&self, name: &str) -> Option<&CommandDef> {
        self.commands.iter().find(|c| c.name == name)
    }
}

/// What a descriptor becomes once its backend has put it somewhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    /// The tool's root on disk. Commands resolve their `run` against it.
    pub root: PathBuf,
}

/// How one kind of tool is fetched and run.
///
/// Every method is an associated function. There is no `self`, because a
/// backend is a policy rather than a value, and keeping it that way is what
/// lets [`Registered::of`] be a `const fn`.
pub trait Backend {
    /// The string a descriptor's `backend =` field names.
    const NAME: &'static str;

    /// The part of a cache key that is about this backend's own environment
    /// rather than about the tool.
    ///
    /// A backend that compiles something returns the toolchain identity here,
    /// so that changing compiler re-keys and forces a coherent rebuild. A
    /// backend that only copies bytes returns the empty string, and should:
    /// keying on something that does not affect the result splits the cache for
    /// no reason.
    fn fingerprint() -> String;

    /// Put the tool's material at `into`, which does not exist yet and whose
    /// parent does.
    ///
    /// Called at most once per cache key. It may take as long as it needs; the
    /// caller has already told the operator that something is being fetched.
    fn materialise(descriptor: &Descriptor, into: &Path) -> Result<(), String>;

    /// Whether a materialised copy is wanted at all.
    ///
    /// `false` for a backend whose source is already on disk, where copying
    /// would put a stale duplicate between an author and their own edits. The
    /// default is `true` and is right for anything fetched.
    fn caches() -> bool {
        true
    }
}

/// A [`Backend`] in the form a host can hold a heterogeneous list of.
///
/// Constructed in a const, so a host's backend table is a `static` rather than
/// something built at start up.
#[derive(Debug, Clone, Copy)]
pub struct Registered {
    /// The name a descriptor matches against.
    pub name:        &'static str,
    fingerprint:     fn() -> String,
    materialise:     fn(&Descriptor, &Path) -> Result<(), String>,
    caches:          fn() -> bool,
}

impl Registered {
    /// The vtable for one backend.
    pub const fn of<B: Backend>() -> Self {
        Self {
            name:        B::NAME,
            fingerprint: B::fingerprint,
            materialise: B::materialise,
            caches:      B::caches,
        }
    }

    /// See [`Backend::fingerprint`].
    pub fn fingerprint(&self) -> String {
        (self.fingerprint)()
    }

    /// See [`Backend::caches`].
    pub fn caches(&self) -> bool {
        (self.caches)()
    }
}

/// The backends a host knows about.
///
/// renki ships [`Git`] and [`Local`]. A host with a backend of its own adds it
/// to its own table; nothing here is a closed set.
#[derive(Debug, Clone, Copy)]
pub struct Registry {
    backends: &'static [Registered],
}

impl Registry {
    /// A registry over a static backend table.
    pub const fn new(backends: &'static [Registered]) -> Self {
        Self { backends }
    }

    /// The backend a descriptor names, if this registry has it.
    pub fn get(&self, name: &str) -> Option<&Registered> {
        self.backends.iter().find(|b| b.name == name)
    }

    /// Every backend name, for a diagnostic that has to say what was available.
    pub fn names(&self) -> Vec<&'static str> {
        self.backends.iter().map(|b| b.name).collect()
    }
}

/// The cache key for one tool: its source, its revision, and whatever its
/// backend says about its own environment.
///
/// The workspace is deliberately absent. Two workspaces naming the same tool at
/// the same revision get the same key and therefore one copy on disk, which is
/// the entire reason a shared cache is worth having.
pub fn cache_key(descriptor: &Descriptor, backend: &Registered) -> String {
    let (url, rev) = match &descriptor.source {
        Source::Git { url, rev } => (url.as_str(), rev.as_str()),
        Source::Path { path } => (path.as_str(), ""),
    };
    let mut h = Fnv::new();
    h.write_field(descriptor.backend.as_str());
    h.write_field(url);
    h.write_field(rev);
    h.write_field(&backend.fingerprint());
    h.hex()
}

/// Materialise a tool, or find it already materialised.
///
/// `workspace` is where a [`Source::Path`] resolves against. `cache` is the
/// directory shared across every workspace on this machine.
///
/// # Why there is no lock
///
/// The launcher half holds none either, and says so, because `cargo install
/// --root` takes cargo's own lock on the install root. That is a true statement
/// about exactly one backend and it stops being true here.
///
/// So this does not take a lock; it makes one unnecessary. The backend
/// materialises into a scratch directory of its own, and the finished directory
/// is then moved into place with a single rename. Two callers racing on the same
/// key both do the work and one rename wins, which costs a duplicated fetch in a
/// rare case. A lock would trade that for a lock file to leak, a stale one to
/// detect, and a failure mode where one crashed process blocks every other.
///
/// **The scratch name is unique per call, not per process**, and that is the
/// load bearing part rather than a detail. A name derived from the process id
/// alone is shared by every thread in it: `locate` takes shared references and
/// is `pub`, so two threads on one key wrote into one scratch, and the published
/// tree was spliced from both fetches while both callers returned `Ok`. The
/// counter below is what makes each caller's scratch its own, and it is why the
/// cleanup on the error and lost-race paths can remove a directory without
/// removing somebody else's work.
pub fn locate(
    descriptor: &Descriptor,
    registry: &Registry,
    workspace: &Path,
    cache: &Path,
) -> Result<Located, String> {
    descriptor.check()?;
    let backend = registry.get(&descriptor.backend).ok_or_else(|| {
        format!(
            "the tool `{}` names the backend `{}`, which is not one of: {}",
            descriptor.name,
            descriptor.backend,
            registry.names().join(", ")
        )
    })?;

    if !backend.caches() {
        let Source::Path { path } = &descriptor.source else {
            return Err(format!(
                "the backend `{}` does not cache, so the tool `{}` must name a path source",
                descriptor.backend, descriptor.name
            ));
        };
        let root = workspace.join(path);
        if !root.is_dir() {
            return Err(format!(
                "the tool `{}` is at {}, which is not there",
                descriptor.name,
                root.display()
            ));
        }
        return Ok(Located { root });
    }

    // A path is relative to a workspace and the cache is shared by all of them,
    // so the two cannot be combined: the key would be a workspace-relative
    // string, and two workspaces each holding a `tools/x` would collide on one
    // entry holding whichever content got there first.
    if let Source::Path { path } = &descriptor.source {
        return Err(format!(
            "the tool `{}` is at the path `{path}`, and the backend `{}` caches; \
             a path is relative to one workspace and the cache is shared by all of them",
            descriptor.name, descriptor.backend
        ));
    }

    let key = cache_key(descriptor, backend);
    let root = cache.join("tools").join(&key);
    if root.is_dir() {
        return Ok(Located { root });
    }

    std::fs::create_dir_all(cache.join("tools"))
        .map_err(|e| format!("could not create the tool cache: {e}"))?;

    // Beside the destination rather than in the system temp directory, so the
    // rename below is within one filesystem and therefore atomic. Unique per
    // call rather than per process, per the note above: the counter is what
    // separates two threads racing on one key.
    static NTH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nth = NTH.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let scratch = cache
        .join("tools")
        .join(format!(".{key}.{}.{nth}", std::process::id()));
    // No pre-emptive remove. The name is this call's alone, so anything already
    // at it would be a collision that cannot happen, and the remove that used to
    // be here is what deleted a sibling thread's in-flight materialise.
    (backend.materialise)(descriptor, &scratch).inspect_err(|_| {
        let _ = std::fs::remove_dir_all(&scratch);
    })?;

    match std::fs::rename(&scratch, &root) {
        Ok(()) => Ok(Located { root }),
        // Lost the race. The winner's copy is the same bytes, so use it.
        Err(_) if root.is_dir() => {
            let _ = std::fs::remove_dir_all(&scratch);
            Ok(Located { root })
        },
        Err(e) => {
            let _ = std::fs::remove_dir_all(&scratch);
            Err(format!("could not place the tool at {}: {e}", root.display()))
        },
    }
}

/// The command line for one of a tool's commands.
///
/// `workspace` reaches the child as `<SHORT>_WORKSPACE` and as its working
/// directory, and that is the load bearing part of this function. A tool's code
/// lives in a cache shared by every workspace on the machine; its data does
/// not. So a tool may never derive a data path from its own location, and the
/// only way it can know which workspace it is acting on is because it was told.
///
/// `short` is the host's own short name, and the variable is derived from it the
/// same way [`Tool::root_env`](crate::Tool::root_env) and its siblings are. It
/// is a parameter rather than a constant because this crate is the launcher
/// framework and not any one launcher: a variable named for one host, welded
/// into a published crate, is that host's policy in everybody else's contract
/// with every child process they spawn.
///
/// `<SHORT>_TOOL_ROOT` reaches the child too, naming the tool's own materialised
/// root, so a command can find files it shipped beside itself.
pub fn command(
    descriptor: &Descriptor,
    located: &Located,
    name: &str,
    short: &str,
    workspace: &Path,
    args: &[String],
) -> Result<Command, String> {
    descriptor.check()?;
    let def = descriptor.command(name).ok_or_else(|| {
        let known: Vec<&str> = descriptor.commands.iter().map(|c| c.name.as_str()).collect();
        if known.is_empty() {
            format!("the tool `{}` offers no commands", descriptor.name)
        } else {
            format!(
                "`{}` has no command `{name}`; it offers: {}",
                descriptor.name,
                known.join(", ")
            )
        }
    })?;

    let exe = located.root.join(&def.run);
    if !exe.is_file() {
        return Err(format!(
            "`{} {name}` should run {}, which is not there",
            descriptor.name,
            exe.display()
        ));
    }
    // The check above established the `run` string cannot escape. This
    // establishes the resolved file does not either, which is a different claim:
    // a symlink inside the tool tree pointing out of it passes every check on
    // the string and lands wherever it points. Both are needed and neither
    // implies the other.
    //
    // The resolved pair is used for the comparison and thrown away. `Command`
    // gets the path as written, because canonicalising rewrites it (`/var`
    // becomes `/private/var` here) and the caller asked to run a file at a
    // place, not at that place's other name.
    let (Ok(real), Ok(root)) = (exe.canonicalize(), located.root.canonicalize()) else {
        return Err(format!(
            "`{} {name}` should run {}, which could not be resolved",
            descriptor.name,
            exe.display()
        ));
    };
    if !real.starts_with(&root) {
        return Err(format!(
            "`{} {name}` resolves to {}, which is outside the tool at {}",
            descriptor.name,
            real.display(),
            root.display()
        ));
    }

    let up = short.to_uppercase();
    let mut cmd = Command::new(&exe);
    cmd.args(args)
        .current_dir(workspace)
        .env(format!("{up}_WORKSPACE"), workspace)
        .env(format!("{up}_TOOL_ROOT"), &located.root);
    Ok(cmd)
}

/// A tool already on disk in the workspace, fetched from nowhere.
///
/// For a tool a workspace writes itself. It is not cached, so an edit is live
/// the moment it is saved rather than the next time a key changes.
pub struct Local;

impl Backend for Local {
    const NAME: &'static str = "local";

    fn fingerprint() -> String {
        String::new()
    }

    fn materialise(_: &Descriptor, _: &Path) -> Result<(), String> {
        Err("the local backend does not materialise".into())
    }

    fn caches() -> bool {
        false
    }
}

/// A tool taken from a git repository at one revision.
pub struct Git;

impl Backend for Git {
    const NAME: &'static str = "git";

    /// Nothing. A checkout is the same bytes whatever is installed on the
    /// machine that took it, so there is no environment to key on.
    fn fingerprint() -> String {
        String::new()
    }

    fn materialise(descriptor: &Descriptor, into: &Path) -> Result<(), String> {
        let Source::Git { url, rev } = &descriptor.source else {
            return Err(format!(
                "the tool `{}` uses the git backend and names no git source",
                descriptor.name
            ));
        };

        eprintln!("fetching the tool `{}` ({rev}) ...", descriptor.name);

        let run = |args: &[&str]| -> Result<(), String> {
            let status = Command::new("git")
                .args(args)
                .status()
                .map_err(|e| format!("could not run git: {e}"))?;
            status
                .success()
                .then_some(())
                .ok_or_else(|| format!("git {} failed", args.join(" ")))
        };

        std::fs::create_dir_all(into)
            .map_err(|e| format!("could not create {}: {e}", into.display()))?;
        let at = into.to_string_lossy().into_owned();

        // Fetch the one revision rather than cloning the history. A tool is
        // pinned to a commit, so everything else in that repository is bytes
        // nobody here will ever read.
        run(&["init", "--quiet", &at])?;
        run(&["-C", &at, "remote", "add", "origin", "--", url])?;
        run(&["-C", &at, "fetch", "--quiet", "--depth", "1", "origin", "--", rev])?;
        run(&["-C", &at, "checkout", "--quiet", "FETCH_HEAD"])?;
        Ok(())
    }
}

/// The backends renki ships. A host may use this table, or build its own that
/// includes these and more.
pub static BUILTIN: &[Registered] = &[Registered::of::<Local>(), Registered::of::<Git>()];

#[cfg(test)]
#[path = "extension_tests.rs"]
mod tests;

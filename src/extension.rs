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
//! #     type Plan = Descriptor;
//! #     fn fingerprint() -> String { String::new() }
//! #     fn materialise(_: &Descriptor, _: &std::path::Path) -> Result<(), String> { Ok(()) }
//! # }
//! static BACKENDS: &[Registered] = &[Registered::of::<Git>()];
//! let registry = Registry::new(BACKENDS);
//! ```

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

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
                // A full object name, which is what git can fetch by: forty hex
                // under sha-1 and sixty-four under sha-256, both of which a
                // repository may use. A short prefix is neither: `git fetch
                // --depth 1 origin <7-hex>` fails with `couldn't find remote
                // ref`, measured against git 2.55, so accepting one only defers
                // the error to the fetch.
                if !rev.chars().all(|c| c.is_ascii_hexdigit())
                    || !matches!(rev.len(), 40 | 64)
                {
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

    /// What this backend is told to materialise.
    ///
    /// A [`Descriptor`] for anything a host dispatches by name out of a
    /// [`Registry`], because a runtime table has to hold one plan type for
    /// every row. The launcher half's [`Cargo`] backend takes a
    /// [`CargoPlan`](crate::CargoPlan) instead: it is chosen at compile time,
    /// so it is reached through [`materialise_once`] directly and never sits in
    /// a registry.
    ///
    /// One contract, two ways of picking the impl: statically where the host
    /// knows which backend it wants, and through the table where it does not.
    type Plan;

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
    fn materialise(plan: &Self::Plan, into: &Path) -> Result<(), String>;

    /// Whether a materialised copy is wanted at all.
    ///
    /// `false` for a backend whose source is already on disk, where copying
    /// would put a stale duplicate between an author and their own edits. The
    /// default is `true` and is right for anything fetched.
    fn caches() -> bool {
        true
    }

    /// Whether [`materialise`](Backend::materialise) writes into its final
    /// destination and holds its own exclusion while it does.
    ///
    /// The default is `false`, and [`materialise_once`] then builds into a
    /// scratch directory and renames the finished tree into place, so a reader
    /// never sees a half-written tool and two callers racing cost one wasted
    /// fetch.
    ///
    /// `true` for a backend that already has a lock of its own over the
    /// destination. [`Cargo`](crate::Cargo) is the case: `cargo install --root`
    /// takes cargo's own lock on the install root and moves the binary in
    /// itself, so a scratch and a rename would add a second, weaker mechanism
    /// on top of a working one, and would throw away cargo's incremental
    /// target directory every time.
    fn places_itself() -> bool {
        false
    }
}

/// Put a backend's material at `root`, once, however that backend places it.
///
/// The launcher's route in, with [`Cargo`](crate::Cargo) named at compile time.
/// [`locate`] reaches the same core through [`place`] instead, because it holds
/// a [`Registered`] rather than a type. Both go through `place`, so a
/// precondition added there reaches both.
pub fn materialise_once<B: Backend>(plan: &B::Plan, root: &Path) -> Result<(), String> {
    place(root, B::places_itself(), |into| B::materialise(plan, into))
}

/// Put material at `root`, once, by whichever of the two routes the backend
/// takes.
///
/// Not generic, so both halves of the crate share one body rather than one
/// shape. `materialise_once` is this with the backend named at compile time;
/// [`locate`] is this with the backend found in a registry, and the duplicate
/// dispatch the two used to carry is what this removes.
///
/// Which mechanism keeps two concurrent callers from tripping over each other is
/// [`Backend::places_itself`], because that is a fact about the backend rather
/// than about the caller. So this takes the answer as a plain argument and is
/// crate-only: public, a host could hand it one the backend contradicts, which
/// is the sentence above stopped being true.
pub(crate) fn place(
    root: &Path,
    places_itself: bool,
    materialise: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    if places_itself {
        std::fs::create_dir_all(root)
            .map_err(|e| format!("could not create {}: {e}", root.display()))?;
        return materialise(root);
    }
    let parent = root
        .parent()
        .ok_or_else(|| format!("{} has no parent", root.display()))?;
    std::fs::create_dir_all(parent)
        .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
    place_via_scratch(root, parent, materialise)
}

/// Build into a scratch beside `root` and rename the finished tree into place.
///
/// Split out of [`materialise_once`] so it is not generic: the body is the same
/// for every backend, and monomorphising it per backend would duplicate it for
/// no reason.
///
/// The scratch name is unique per call rather than per process, since `locate`
/// takes shared references and is `pub`, so several threads reach one key at
/// once. That is also what lets the cleanup paths remove a directory without
/// touching another caller's.
fn place_via_scratch(
    root: &Path,
    parent: &Path,
    materialise: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    static NTH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nth = NTH.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let leaf = root.file_name().unwrap_or_default().to_string_lossy();
    // Beside the destination rather than in the system temp directory, so the
    // rename is within one filesystem and therefore atomic.
    let scratch = parent.join(format!(".{leaf}.{}.{nth}", std::process::id()));
    // No pre-emptive remove. The name is this call's alone, so anything already
    // at it would be a collision that cannot happen, and the remove that used to
    // be here is what deleted a sibling thread's in-flight materialise.
    materialise(&scratch).inspect_err(|_| {
        let _ = std::fs::remove_dir_all(&scratch);
    })?;

    match std::fs::rename(&scratch, root) {
        Ok(()) => Ok(()),
        // Lost the race. The winner's copy is the same bytes, so use it.
        Err(_) if root.is_dir() => {
            let _ = std::fs::remove_dir_all(&scratch);
            Ok(())
        },
        Err(e) => {
            let _ = std::fs::remove_dir_all(&scratch);
            Err(format!("could not place the tool at {}: {e}", root.display()))
        },
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
    places_itself:   fn() -> bool,
}

impl Registered {
    /// The vtable for one backend.
    ///
    /// Only a backend whose [`Plan`](Backend::Plan) is a [`Descriptor`] can go
    /// in a registry, because a runtime table dispatched by name has to hold
    /// one plan type across every row. A backend planned on something else is
    /// reached statically instead, through [`materialise_once`].
    pub const fn of<B: Backend<Plan = Descriptor>>() -> Self {
        Self {
            name:        B::NAME,
            fingerprint: B::fingerprint,
            materialise: B::materialise,
            caches:      B::caches,
            places_itself: B::places_itself,
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

    /// What [`Backend::places_itself`] says.
    pub fn places_itself(&self) -> bool {
        (self.places_itself)()
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
/// what a shared cache is for.
///
/// # Errors
///
/// A [`Source::Path`], because a path is relative to one workspace and the key
/// is not, so keying on one would put two workspaces each holding a `tools/x`
/// on a single entry. [`locate`] refuses the same shape and this refuses it too:
/// this is `pub`, so a host reaching it directly would otherwise get the exact
/// collision `locate` exists to prevent.
pub fn cache_key(descriptor: &Descriptor, backend: &Registered) -> Result<String, String> {
    let Source::Git { url, rev } = &descriptor.source else {
        return Err(format!(
            "the tool `{}` names a path, which is relative to one workspace, \
             and a cache key is shared by all of them",
            descriptor.name
        ));
    };
    let (url, rev) = (url.as_str(), rev.as_str());
    let mut h = Fnv::new();
    h.write_field(descriptor.backend.as_str());
    h.write_field(url);
    h.write_field(rev);
    h.write_field(&backend.fingerprint());
    Ok(h.hex())
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
/// The scratch is named per call rather than per process, since this takes
/// shared references and is `pub`, so several threads reach one key at once and
/// a per-process name would be shared between them.
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

    // This is where a caching backend with a path source is refused, since a
    // path is relative to one workspace and a key is shared by all of them.
    let key = cache_key(descriptor, backend)?;
    let root = cache.join("tools").join(&key);
    if root.is_dir() {
        // On the hit rather than only on the fetch, which is the whole point:
        // [`collect`] ages a tool out on last use, and a marker written once at
        // fetch time would say a tool used every day had not been touched since
        // the day it arrived.
        touch(&root);
        return Ok(Located { root });
    }

    place(&root, backend.places_itself(), |into| {
        (backend.materialise)(descriptor, into)
    })?;
    touch(&root);
    Ok(Located { root })
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

    type Plan = Descriptor;

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

    type Plan = Descriptor;

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

/// What a [`Cargo`] build is told to install.
///
/// `attempts` are argument lists tried in order until one succeeds, source
/// selector and package name included; `--root` and `--force` are added here.
/// A version pin has two, the registry and then the matching git tag.
#[derive(Debug, Clone)]
pub struct CargoPlan {
    /// The argument lists, tried in order.
    pub attempts: Vec<Vec<String>>,
    /// The binary the install must produce under `<root>/bin`, checked because
    /// cargo reporting success is not the same as cargo having built the thing
    /// that was wanted.
    pub bin:      String,
    /// Named in the failure message, so an operator reads the crate that did
    /// not build rather than the launcher that asked for it.
    pub crate_name: String,
}

/// Building something with cargo.
///
/// The launcher half's backend, and the reason [`Backend`] is a contract rather
/// than the one thing this crate happens to do. It is picked at compile time by
/// the launcher, so it goes through [`materialise_once`] directly and is not in
/// any [`Registry`]: its [`Plan`](Backend::Plan) is a [`CargoPlan`] rather than
/// a [`Descriptor`], and a table dispatched by a descriptor's `backend =` field
/// has nothing to hand it.
pub struct Cargo;

impl Backend for Cargo {
    const NAME: &'static str = "cargo";

    type Plan = CargoPlan;

    /// `rustc -vV`, which carries the version, the commit hash, the host triple
    /// and the LLVM version.
    ///
    /// rustc is part of the compilation input, so a toolchain change re-keys and
    /// forces a coherent rebuild. A frozen binary paired with a moved toolchain
    /// is the failure this prevents.
    fn fingerprint() -> String {
        crate::cache::rustc_fingerprint()
    }

    /// `cargo install --root <into> --force`, per attempt, first success wins.
    fn materialise(plan: &Self::Plan, into: &Path) -> Result<(), String> {
        let mut failures = Vec::new();
        for attempt in &plan.attempts {
            let status = Command::new("cargo")
                .arg("install")
                .args(attempt)
                .arg("--root")
                .arg(into)
                .arg("--force")
                .status()
                .map_err(|e| format!("could not run cargo install: {e}"))?;
            if !status.success() {
                failures.push(format!("{attempt:?} failed"));
                continue;
            }
            if into.join("bin").join(&plan.bin).is_file() {
                return Ok(());
            }
            failures.push(format!(
                "{attempt:?} reported success but produced no binary"
            ));
        }
        Err(crate::cache::build_failure(&plan.crate_name, &failures))
    }

    /// `cargo install --root` takes cargo's own lock on the install root and
    /// moves the binary in itself, so a scratch and a rename would stack a
    /// second, weaker mechanism on a working one and throw away the incremental
    /// target directory on every build.
    fn places_itself() -> bool {
        true
    }
}

/// The file whose timestamp says when a materialised tool was last used.
///
/// The directory's own timestamp will not do. A directory's modification time
/// moves when an entry is added to it or removed from it, and not when
/// something inside an entry is read, so a tool used daily would carry the time
/// it was fetched and be swept as though it had never been touched.
const USED_MARKER: &str = ".last-used";

/// Record that a tool is in use, for [`collect`] to read later.
///
/// Best effort. A marker that cannot be written costs a re-fetch after the
/// retention window and nothing else, so it is not worth failing a run over.
fn touch(root: &Path) {
    let _ = std::fs::write(root.join(USED_MARKER), b"");
}

/// Remove materialised tools nothing has used within `retention`, and any
/// scratch left by a fetch that died partway.
///
/// Returns what went, for the caller to report.
///
/// Nothing else collects `<cache>/tools`. The build registry knows which repo
/// pins which engine and can evict on that; there is no equivalent record for
/// tools, because a tool is named by a workspace's own configuration rather
/// than resolved through a pin. So this ages them out on last use instead,
/// which is the same mechanism the scratch engine builds already use and needs
/// no bookkeeping to stay correct.
///
/// A scratch is taken on a shorter rule, and the rule is a time bound rather
/// than a liveness test. Nothing here asks whether the fetch that owns one is
/// still running: an hour is far longer than a fetch is expected to take and
/// short enough that a crashed run does not leave a copy of a repository until
/// the retention window, and that is the whole of the argument.
///
/// The bound is measured on the scratch directory's own timestamp, which does
/// not move when something nested inside it is written. A backend that creates
/// the directory and then works inside a subtree of it, as `git init` followed
/// by a fetch does, carries the timestamp of the creation for the whole
/// download. So a fetch slow enough to cross the hour has its tree taken from
/// under it. It fails rather than publishing a partial tool, and it is a real
/// cost of the bound rather than something the bound avoids.
///
/// **A scratch belonging to this process is never taken.** That is the one case
/// liveness can be settled cheaply, since the pid is in the name, and it is also
/// the case that would otherwise bite: a launcher collects on the same run that
/// fetches, so a slow fetch could be collected by its own process.
pub fn collect(cache: &Path, retention: std::time::Duration, now: SystemTime) -> Vec<String> {
    const SCRATCH_TTL: std::time::Duration = std::time::Duration::from_secs(60 * 60);

    let dir = cache.join("tools");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut removed = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let scratch = name.starts_with('.');
        // Ours, and therefore live. A launcher collects on the same run that
        // fetches, so without this a slow fetch is collected by its own process.
        if scratch && name.contains(&format!(".{}.", std::process::id())) {
            continue;
        }
        let ttl = if scratch { SCRATCH_TTL } else { retention };

        // A tool's age is its marker's; a scratch has none and is judged on the
        // directory, which is right there because a scratch is only ever
        // written to during the one fetch that owns it.
        let stamp = if scratch {
            path.metadata().and_then(|m| m.modified()).ok()
        } else {
            path.join(USED_MARKER)
                .metadata()
                .and_then(|m| m.modified())
                .ok()
        };
        // No marker at all means it predates this mechanism, so it is stamped
        // now and swept on a later pass if it stays unused. Deleting it here
        // would evict every tool on disk the first time a launcher with this
        // code runs.
        let Some(stamp) = stamp else {
            if !scratch {
                touch(&path);
            }
            continue;
        };
        if now.duration_since(stamp).is_ok_and(|age| age > ttl) {
            let _ = std::fs::remove_dir_all(&path);
            removed.push(name);
        }
    }
    removed
}

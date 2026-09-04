// A function that wants the cache root cannot be handed the state root, nor a
// root resolved for the other platform. The type is the guard, and this file
// is what keeps it one.
//
// Both platforms are named rather than `Host`, because the expected diagnostic
// is committed beside this file and `Host` expands differently per build host,
// which would make the suite red on whichever platform did not write it.
use notko::Maybe;
use renki_dirs::{Cache, MacOs, Namespace, Root, Sources, State, Xdg};

fn wants_cache(_: Root<'_, Cache, Xdg>) {}
fn wants_xdg(_: Root<'_, Cache, Xdg>) {}

fn main() {
    let ns = Namespace::new("t").unwrap();
    let s = Sources {
        own: Maybe::Isnt,
        xdg: Maybe::Isnt,
        home: Maybe::Is("/home/u"),
    };
    wants_cache(Root::<State, Xdg>::resolve(ns, s).unwrap());
    wants_xdg(Root::<Cache, MacOs>::resolve(ns, s).unwrap());
}

// A function that wants the cache root cannot be handed the state root, nor a
// root resolved for the other platform. The type is the guard, and this file
// is what keeps it one.
use notko::Maybe;
use renki_dirs::{Cache, Host, MacOs, Namespace, Root, Sources, State, Xdg};

fn wants_cache(_: Root<'_, Cache, Host>) {}
fn wants_xdg(_: Root<'_, Cache, Xdg>) {}

fn main() {
    let ns = Namespace::new("t").unwrap();
    let s = Sources {
        own: Maybe::Isnt,
        xdg: Maybe::Isnt,
        home: Maybe::Is("/home/u"),
    };
    wants_cache(Root::<State, Host>::resolve(ns, s).unwrap());
    wants_xdg(Root::<Cache, MacOs>::resolve(ns, s).unwrap());
}

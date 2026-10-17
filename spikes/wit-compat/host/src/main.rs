//! Evidence for ADR 0012. Run `bash spikes/wit-compat/run.sh`.
//!
//!   spike-wit-compat <guest-variant.wasm> <guest-semver.wasm>

use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Engine, Store};

/// Host 1.1.0 that adds a variant case and a function.
mod case {
    wasmtime::component::bindgen!({ world: "host", path: "../wit-v2-case" });
}
/// Host 1.1.0 that adds a function only.
mod func {
    wasmtime::component::bindgen!({ world: "host", path: "../wit-v2-func" });
}
/// Host 0.1.1 that adds a function only.
mod semver {
    wasmtime::component::bindgen!({ world: "host", path: "../wit-011" });
}

struct S;
impl case::test::api::edit::Host for S {
    fn submit(&mut self, cmds: Vec<case::test::api::edit::Command>) -> u32 {
        cmds.len() as u32
    }
    fn extra(&mut self) -> u32 {
        7
    }
}
impl func::test::api::edit::Host for S {
    fn submit(&mut self, cmds: Vec<func::test::api::edit::Command>) -> u32 {
        cmds.len() as u32
    }
    fn submit_c(&mut self, _flag: bool) -> u32 {
        9
    }
}
impl semver::test::api::types::Host for S {}
impl semver::test::api::used::Host for S {
    fn fa(&mut self, p: semver::test::api::types::Point) -> f64 {
        p.x + p.y
    }
    fn fc(&mut self) -> u32 {
        3
    }
}
impl semver::test::api::unused::Host for S {
    fn fb(&mut self) -> u32 {
        2
    }
}

fn try_host<
    R: std::fmt::Debug
        + wasmtime::component::ComponentType
        + wasmtime::component::Lift
        + Send
        + Sync
        + 'static,
>(
    name: &str,
    engine: &Engine,
    component: &Component,
    linker: Linker<S>,
) {
    let mut store = Store::new(engine, S);
    match linker.instantiate(&mut store, component) {
        Ok(instance) => {
            let run = instance
                .get_typed_func::<(), (R,)>(&mut store, "run")
                .unwrap();
            println!(
                "{name}: loads, run() = {:?}",
                run.call(&mut store, ()).map(|r| r.0)
            );
        }
        Err(e) => println!("{name}: REFUSED: {e}"),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let engine = Engine::default();

    let variant_guest = Component::from_file(&engine, &args[1]).unwrap();
    let mut l: Linker<S> = Linker::new(&engine);
    case::Host_::add_to_linker::<S, HasSelf<S>>(&mut l, |s| s).unwrap();
    try_host::<u32>(
        "1.0.0 guest, 1.1.0 host with a new variant case",
        &engine,
        &variant_guest,
        l,
    );
    let mut l: Linker<S> = Linker::new(&engine);
    func::Host_::add_to_linker::<S, HasSelf<S>>(&mut l, |s| s).unwrap();
    try_host::<u32>(
        "1.0.0 guest, 1.1.0 host with a new function",
        &engine,
        &variant_guest,
        l,
    );

    let semver_guest = Component::from_file(&engine, &args[2]).unwrap();
    let mut l: Linker<S> = Linker::new(&engine);
    semver::Host_::add_to_linker::<S, HasSelf<S>>(&mut l, |s| s).unwrap();
    try_host::<f64>(
        "0.1.0 guest, 0.1.1 host with a new function",
        &engine,
        &semver_guest,
        l,
    );
}

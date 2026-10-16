//! Spike 3: wasmtime component host.
//! Checks: imports before instantiation, memory limit, epoch interruption,
//! survival after a trap, a failure counter, and the call cost.
//!
//! Run `bash wasm-host/build-guest.sh` first.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use wasmtime::component::{Component, HasSelf, Linker, types::ComponentItem};
use wasmtime::{Config, Engine, Error, Result, Store, StoreLimits, StoreLimitsBuilder};

wasmtime::component::bindgen!({ world: "spike", path: "wit" });

const MEMORY_LIMIT: usize = 64 << 20;
const EPOCH_TICK: Duration = Duration::from_millis(1);
const CALL_DEADLINE_TICKS: u64 = 100;

struct State {
    limits: StoreLimits,
    logs: Vec<String>,
}

impl spike::api::host::Host for State {
    fn log(&mut self, msg: String) {
        self.logs.push(msg);
    }

    fn page_count(&mut self) -> u32 {
        50
    }
}

fn report(name: &str, ok: bool, detail: impl AsRef<str>) {
    let mark = if ok { "PASS" } else { "FAIL" };
    println!("[{mark}] {name}: {}", detail.as_ref());
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// Short reason for a failed call: the trap code if there is one.
fn cause(e: &Error) -> String {
    match e.downcast_ref::<wasmtime::Trap>() {
        Some(trap) => format!("trap: {trap}"),
        None => e.to_string().lines().next().unwrap_or("").to_string(),
    }
}

fn out_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../out")
}

/// One loaded mod: a fresh store and instance per load, plus a failure counter.
struct ModSlot<'a> {
    engine: &'a Engine,
    linker: &'a Linker<State>,
    component: &'a Component,
    store: Store<State>,
    bindings: Spike,
    failures: u32,
    disabled: bool,
}

impl<'a> ModSlot<'a> {
    fn load(
        engine: &'a Engine,
        linker: &'a Linker<State>,
        component: &'a Component,
    ) -> Result<Self> {
        let (store, bindings) = instantiate(engine, linker, component)?;
        Ok(Self {
            engine,
            linker,
            component,
            store,
            bindings,
            failures: 0,
            disabled: false,
        })
    }

    /// Runs one guest call with a fresh deadline. A failure poisons the instance, so reload it.
    fn call<T>(&mut self, f: impl FnOnce(&Spike, &mut Store<State>) -> Result<T>) -> Result<T> {
        if self.disabled {
            return Err(Error::msg("mod is disabled"));
        }
        self.store.set_epoch_deadline(CALL_DEADLINE_TICKS);
        match f(&self.bindings, &mut self.store) {
            Ok(v) => {
                self.failures = 0;
                Ok(v)
            }
            Err(e) => {
                self.failures += 1;
                if self.failures >= 3 {
                    self.disabled = true;
                }
                let (store, bindings) = instantiate(self.engine, self.linker, self.component)?;
                (self.store, self.bindings) = (store, bindings);
                Err(e)
            }
        }
    }
}

fn instantiate(
    engine: &Engine,
    linker: &Linker<State>,
    component: &Component,
) -> Result<(Store<State>, Spike)> {
    let state = State {
        limits: StoreLimitsBuilder::new().memory_size(MEMORY_LIMIT).build(),
        logs: vec![],
    };
    let mut store = Store::new(engine, state);
    store.limiter(|s| &mut s.limits);
    store.epoch_deadline_trap();
    store.set_epoch_deadline(CALL_DEADLINE_TICKS);
    let bindings = Spike::instantiate(&mut store, component, linker)?;
    Ok((store, bindings))
}

fn imports(engine: &Engine, component: &Component) -> Vec<String> {
    component
        .component_type()
        .imports(engine)
        .map(|(name, item)| {
            let kind = match item.ty {
                ComponentItem::ComponentInstance(_) => "instance",
                ComponentItem::ComponentFunc(_) => "func",
                _ => "other",
            };
            format!("{name} ({kind})")
        })
        .collect()
}

fn main() -> Result<()> {
    let mut config = Config::new();
    config.epoch_interruption(true);
    let engine = Engine::new(&config)?;

    // Epoch ticker. One tick per millisecond.
    let stop = Arc::new(AtomicBool::new(false));
    let ticker = {
        let (engine, stop) = (engine.clone(), stop.clone());
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(EPOCH_TICK);
                engine.increment_epoch();
            }
        })
    };

    let mut linker: Linker<State> = Linker::new(&engine);
    Spike::add_to_linker::<State, HasSelf<State>>(&mut linker, |s| s)?;

    // 1. Read imports before instantiation, for both guest builds.
    let bytes = std::fs::read(out_dir().join("guest-unknown.wasm"))?;
    let started = Instant::now();
    let component = Component::new(&engine, &bytes)?;
    let compile_ms = ms(started.elapsed());
    let list = imports(&engine, &component);
    report(
        "imports before instantiation (wasm32-unknown-unknown guest)",
        list == ["spike:api/host@0.1.0 (instance)"],
        format!(
            "{list:?}; compile {compile_ms:.1} ms for {} KB",
            bytes.len() / 1024
        ),
    );

    let wasi = Component::new(&engine, std::fs::read(out_dir().join("guest-wasip2.wasm"))?)?;
    let wasi_list = imports(&engine, &wasi);
    let refused = instantiate(&engine, &linker, &wasi).err();
    report(
        "WASI guest is refused without WASI in the linker",
        refused.is_some(),
        format!(
            "{} imports, {} of them wasi:*; error: {}",
            wasi_list.len(),
            wasi_list.iter().filter(|i| i.starts_with("wasi:")).count(),
            refused.map(|e| e.to_string()).unwrap_or_default()
        ),
    );

    // 2. Normal call, host import, and the call cost.
    let started = Instant::now();
    let mut slot = ModSlot::load(&engine, &linker, &component)?;
    let instantiate_ms = ms(started.elapsed());
    let value = slot.call(|b, s| b.call_run(s, 0))?;
    let err = slot.call(|b, s| b.call_run(s, u32::MAX))?;
    report(
        "call and host import",
        value == Ok(50) && err.is_err() && slot.store.data().logs == ["run called with 0"],
        format!("run(0) = {value:?}, run(MAX) = {err:?}, instantiate {instantiate_ms:.2} ms"),
    );
    let n = 100_000;
    let started = Instant::now();
    for i in 1..=n {
        slot.call(|b, s| b.call_run(s, i))?.unwrap();
    }
    let per_call_us = started.elapsed().as_secs_f64() * 1e6 / n as f64;
    report(
        "call cost",
        true,
        format!("{per_call_us:.2} us per call, including one host import call ({n} calls)"),
    );

    // 3. Memory limit.
    let small = slot.call(|b, s| b.call_grow_memory(s, 16));
    let big = slot.call(|b, s| b.call_grow_memory(s, 128));
    report(
        "memory limit (64 MB)",
        matches!(small, Ok(16)) && big.is_err(),
        format!(
            "16 MB: {small:?}; 128 MB: {}",
            big.err().map(|e| cause(&e)).unwrap_or_default()
        ),
    );

    // 4. Epoch interruption stops an endless loop.
    let started = Instant::now();
    let spin = slot.call(|b, s| b.call_spin(s));
    let stop_ms = ms(started.elapsed());
    report(
        "epoch interruption stops an endless loop",
        spin.is_err() && stop_ms < 500.0,
        format!(
            "deadline {CALL_DEADLINE_TICKS} ticks of 1 ms, stopped after {stop_ms:.1} ms: {}",
            spin.err().map(|e| cause(&e)).unwrap_or_default()
        ),
    );

    // 5. The host survives a trap, and a fresh instance works.
    let mut slot = ModSlot::load(&engine, &linker, &component)?;
    let trap = slot.call(|b, s| b.call_trap(s));
    let after = slot.call(|b, s| b.call_run(s, 1));
    report(
        "host survives a trap",
        trap.is_err() && matches!(after, Ok(Ok(51))),
        format!(
            "{}; next call on a fresh instance: {after:?}",
            trap.err().map(|e| cause(&e)).unwrap_or_default()
        ),
    );

    // 6. Three failures in a row disable the mod.
    let mut slot = ModSlot::load(&engine, &linker, &component)?;
    for _ in 0..3 {
        let _ = slot.call(|b, s| b.call_trap(s));
    }
    let refused = slot.call(|b, s| b.call_run(s, 1));
    report(
        "failure counter disables the mod after 3 failures",
        slot.disabled && refused.is_err(),
        format!(
            "failures {}, next call: {}",
            slot.failures,
            refused.err().map(|e| e.to_string()).unwrap_or_default()
        ),
    );

    stop.store(true, Ordering::Relaxed);
    ticker.join().unwrap();
    Ok(())
}

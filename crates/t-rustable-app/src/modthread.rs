//! The thread that owns the `ModHost`.
//!
//! `ModHost` methods take `&mut self`, and `ui.confirm` and `files.pick` wait for the
//! user. So all mod calls run on this one thread, and Tauri commands wait for the reply
//! on a blocking pool thread, never on the main thread.

use std::sync::mpsc;
use t_rustable_plugin_host::ModHost;

type Job = Box<dyn FnOnce(&mut ModHost) + Send>;

pub struct ModThread {
    tx: mpsc::Sender<Job>,
}

impl ModThread {
    pub fn start(make: impl FnOnce() -> ModHost + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("mods".into())
            .spawn(move || {
                let mut host = make();
                for job in rx {
                    job(&mut host);
                }
            })
            .expect("start the mod thread");
        Self { tx }
    }

    /// Runs `f` on the mod thread. The caller waits on a blocking pool thread.
    pub async fn call<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut ModHost) -> R + Send + 'static,
    ) -> Result<R, String> {
        let (rtx, rrx) = mpsc::channel();
        self.tx
            .send(Box::new(move |host| {
                let _ = rtx.send(f(host));
            }))
            .map_err(|_| "The mod thread stopped.".to_string())?;
        tauri::async_runtime::spawn_blocking(move || rrx.recv())
            .await
            .map_err(|e| e.to_string())?
            .map_err(|_| "The mod thread stopped.".to_string())
    }
}

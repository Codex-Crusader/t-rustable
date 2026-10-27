//! The engine thread: one thread owns all PDFium calls (ARCHITECTURE section 2).

use crate::handler::{self, State};
use crate::pdfium::Pdfium;
use crate::protocol::{EngineError, Priority, Request, Response};
use std::collections::{HashMap, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;

struct Job {
    req: Request,
    cancel: Arc<AtomicBool>,
    reply: Sender<Result<Response, EngineError>>,
}

#[derive(Default)]
struct Queues {
    by_priority: [VecDeque<Job>; 3],
    stop: bool,
}

impl Queues {
    fn pop(&mut self) -> Option<Job> {
        self.by_priority.iter_mut().find_map(VecDeque::pop_front)
    }
}

#[derive(Default)]
struct Shared {
    queues: Mutex<Queues>,
    wake: Condvar,
    next_id: AtomicU64,
    restarts: AtomicU64,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Queues> {
        self.queues.lock().unwrap_or_else(|e| e.into_inner())
    }
}

struct Inner {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.shared.lock().stop = true;
        self.shared.wake.notify_all();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// A handle to the engine thread. Clones share the thread. The thread stops when the last
/// handle drops.
#[derive(Clone)]
pub struct Engine {
    inner: Arc<Inner>,
}

/// A request in the queue. Exactly one reply comes for each ticket.
pub struct Ticket {
    pub id: u64,
    cancel: Arc<AtomicBool>,
    rx: Receiver<Result<Response, EngineError>>,
}

impl Ticket {
    /// Sets the cancel flag. A queued request does not start. A render stops at its next pause.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        self.cancel.clone()
    }

    pub fn wait(self) -> Result<Response, EngineError> {
        // The sender drops without a reply only when the engine stops.
        self.rx.recv().unwrap_or(Err(EngineError::Cancelled))
    }
}

impl Engine {
    /// Starts the engine thread and loads PDFium from `library` (the `pdfium.dll` or
    /// `libpdfium.so` file).
    pub fn start(library: &Path) -> Result<Self, EngineError> {
        let shared = Arc::new(Shared::default());
        let (ready_tx, ready_rx) = mpsc::channel();
        let library = library.to_path_buf();
        let thread_shared = shared.clone();
        let thread = std::thread::Builder::new()
            .name("t-rustable-engine".into())
            .spawn(move || {
                let pdfium = match Pdfium::load(&library) {
                    Ok(p) => p,
                    Err(e) => {
                        let _ = ready_tx.send(Err(EngineError::Internal(e)));
                        return;
                    }
                };
                let _ = ready_tx.send(Ok(()));
                run(
                    &thread_shared,
                    State {
                        pdfium,
                        docs: HashMap::new(),
                    },
                );
            })
            .map_err(|e| EngineError::Internal(format!("cannot start the engine thread: {e}")))?;
        let inner = Inner {
            shared,
            thread: Some(thread),
        };
        ready_rx
            .recv()
            .unwrap_or_else(|_| Err(EngineError::Internal("the engine thread stopped".into())))?;
        Ok(Self {
            inner: Arc::new(inner),
        })
    }

    pub fn submit(&self, priority: Priority, req: Request) -> Ticket {
        let shared = &self.inner.shared;
        let id = shared.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let cancel = Arc::new(AtomicBool::new(false));
        let (reply, rx) = mpsc::channel();
        let job = Job {
            req,
            cancel: cancel.clone(),
            reply,
        };
        shared.lock().by_priority[priority as usize].push_back(job);
        shared.wake.notify_one();
        Ticket { id, cancel, rx }
    }

    /// Submits a request and waits for its reply.
    pub fn call(&self, priority: Priority, req: Request) -> Result<Response, EngineError> {
        self.submit(priority, req).wait()
    }

    /// How many times the engine restarted PDFium after a panic.
    pub fn restarts(&self) -> u64 {
        self.inner.shared.restarts.load(Ordering::Relaxed)
    }
}

fn run(shared: &Shared, mut state: State) {
    loop {
        let job = {
            let mut q = shared.lock();
            loop {
                if q.stop {
                    return;
                }
                if let Some(job) = q.pop() {
                    break job;
                }
                q = shared.wake.wait(q).unwrap_or_else(|e| e.into_inner());
            }
        };
        let result = if job.cancel.load(Ordering::Relaxed) {
            Err(EngineError::Cancelled)
        } else {
            let cancel = job.cancel.clone();
            match catch_unwind(AssertUnwindSafe(|| {
                handler::handle(&mut state, job.req, &cancel)
            })) {
                Ok(r) => r,
                Err(_) => {
                    // The panic hook of the front end writes the panic message.
                    log::error!(
                        "PDFium panicked. The engine closes all documents and starts it again."
                    );
                    restart(&mut state);
                    shared.restarts.fetch_add(1, Ordering::Relaxed);
                    Err(EngineError::Restarted)
                }
            }
        };
        let _ = job.reply.send(result);
    }
}

/// After a panic, PDFium state can be bad. Close all documents and start PDFium again
/// (ADR 0013). The core reloads its sessions from their journals.
fn restart(state: &mut State) {
    let docs = std::mem::take(&mut state.docs);
    let _ = catch_unwind(AssertUnwindSafe(move || drop(docs)));
    state.pdfium.restart();
}

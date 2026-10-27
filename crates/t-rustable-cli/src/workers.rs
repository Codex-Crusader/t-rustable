//! `--jobs N`: N worker processes, each with its own engine and mod host (ADR 0020).
//!
//! The parent starts this program again with the hidden `worker` command. It talks to
//! each worker through stdin and stdout, one JSON value per line:
//!
//! 1. The parent sends the plan. The worker answers `{"ready": true}`, or
//!    `{"ready": false, "error": ...}` if it cannot start.
//! 2. The parent sends one job. The worker answers with the result of that file.
//! 3. The parent closes stdin. The worker stops.
//!
//! If a worker stops during a job, that file fails, and the parent starts a new worker.

use std::collections::{BTreeMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitCode, Stdio};
use std::sync::{Arc, Mutex, mpsc};

use serde::{Deserialize, Serialize};

use crate::report::{FileResult, Problem};
use crate::run::{Job, Plan, Runner};

#[derive(Serialize, Deserialize)]
struct Ready {
    ready: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<Problem>,
}

fn send<T: Serialize>(out: &mut impl Write, value: &T) -> std::io::Result<()> {
    let mut line = serde_json::to_string(value).map_err(std::io::Error::other)?;
    line.push('\n');
    out.write_all(line.as_bytes())?;
    out.flush()
}

/// The hidden `worker` command. Only protocol lines go to stdout.
pub fn worker_main() -> ExitCode {
    let mut stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    let mut line = String::new();
    let plan: Plan = match stdin
        .read_line(&mut line)
        .map(|_| serde_json::from_str(&line))
    {
        Ok(Ok(plan)) => plan,
        _ => return ExitCode::from(crate::report::USAGE),
    };
    crate::report::init_logger(plan.verbose);
    let mut runner = match Runner::new(plan) {
        Ok(r) => r,
        Err(e) => {
            let _ = send(
                &mut stdout,
                &Ready {
                    ready: false,
                    error: Some(e),
                },
            );
            return ExitCode::from(crate::report::USAGE);
        }
    };
    if send(
        &mut stdout,
        &Ready {
            ready: true,
            error: None,
        },
    )
    .is_err()
    {
        return ExitCode::FAILURE;
    }
    loop {
        line.clear();
        match stdin.read_line(&mut line) {
            Ok(0) | Err(_) => return ExitCode::SUCCESS,
            Ok(_) => {}
        }
        let Ok(job) = serde_json::from_str::<Job>(&line) else {
            return ExitCode::from(crate::report::USAGE);
        };
        if send(&mut stdout, &runner.run(&job)).is_err() {
            return ExitCode::FAILURE;
        }
    }
}

struct Worker {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Worker {
    fn start(plan: &Plan) -> Result<Self, Problem> {
        let exe = std::env::current_exe()
            .map_err(|e| Problem::new("internal", format!("Cannot find this program: {e}")))?;
        let mut child = Command::new(exe)
            .arg("worker")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| Problem::new("internal", format!("Cannot start a worker: {e}")))?;
        let mut w = Self {
            stdin: child.stdin.take().unwrap(),
            stdout: BufReader::new(child.stdout.take().unwrap()),
            child,
        };
        send(&mut w.stdin, plan).map_err(|e| w.stopped(e))?;
        match w.read::<Ready>() {
            Ok(Ready { ready: true, .. }) => Ok(w),
            Ok(Ready { error: Some(e), .. }) => Err(e),
            Ok(_) => Err(Problem::new("internal", "The worker did not start.")),
            Err(e) => Err(e),
        }
    }

    fn stopped(&mut self, e: impl std::fmt::Display) -> Problem {
        let _ = self.child.kill();
        let _ = self.child.wait();
        Problem::new("worker-stopped", format!("The worker process stopped: {e}"))
    }

    fn read<T: for<'de> Deserialize<'de>>(&mut self) -> Result<T, Problem> {
        let mut line = String::new();
        match self.stdout.read_line(&mut line) {
            Ok(0) => Err(self.stopped("no answer")),
            Ok(_) => serde_json::from_str(&line).map_err(|e| self.stopped(e)),
            Err(e) => Err(self.stopped(e)),
        }
    }

    fn run(&mut self, job: &Job) -> Result<FileResult, Problem> {
        send(&mut self.stdin, job).map_err(|e| self.stopped(e))?;
        self.read()
    }

    fn finish(self) {
        drop(self.stdin);
        let mut child = self.child;
        let _ = child.wait();
    }
}

enum Msg {
    Ready(Result<(), Problem>),
    Done(usize, FileResult),
}

/// Runs the jobs on `n` workers. `on_result` gets the results in input order. An error
/// means that a worker could not start, and no job ran.
pub fn run_all(
    plan: &Plan,
    jobs: Vec<Job>,
    n: usize,
    mut on_result: impl FnMut(&FileResult),
) -> Result<Vec<FileResult>, Problem> {
    let total = jobs.len();
    let n = n.min(total).max(1);
    let queue = Arc::new(Mutex::new(
        jobs.into_iter().enumerate().collect::<VecDeque<_>>(),
    ));
    let (tx, rx) = mpsc::channel();
    let mut go = Vec::new();
    let mut threads = Vec::new();
    for _ in 0..n {
        let (go_tx, go_rx) = mpsc::channel::<bool>();
        go.push(go_tx);
        let (plan, queue, tx) = (plan.clone(), queue.clone(), tx.clone());
        threads.push(std::thread::spawn(move || {
            let mut worker = match Worker::start(&plan) {
                Ok(w) => {
                    let _ = tx.send(Msg::Ready(Ok(())));
                    Some(w)
                }
                Err(e) => {
                    let _ = tx.send(Msg::Ready(Err(e)));
                    None
                }
            };
            if go_rx.recv() != Ok(true) {
                if let Some(w) = worker {
                    w.finish();
                }
                return;
            }
            loop {
                let Some((i, job)) = queue.lock().unwrap().pop_front() else {
                    break;
                };
                let w = match worker.as_mut() {
                    Some(w) => w,
                    None => match Worker::start(&plan) {
                        Ok(w) => worker.insert(w),
                        Err(e) => {
                            let _ = tx.send(Msg::Done(i, FileResult::failed(job.input, e)));
                            continue;
                        }
                    },
                };
                let result = w.run(&job).unwrap_or_else(|e| {
                    worker = None;
                    FileResult::failed(job.input.clone(), e)
                });
                let _ = tx.send(Msg::Done(i, result));
            }
            if let Some(w) = worker {
                w.finish();
            }
        }));
    }
    drop(tx);

    let mut problem = None;
    for _ in 0..n {
        if let Ok(Msg::Ready(Err(e))) = rx.recv() {
            problem.get_or_insert(e);
        }
    }
    for g in &go {
        let _ = g.send(problem.is_none());
    }
    if let Some(e) = problem {
        for t in threads {
            let _ = t.join();
        }
        return Err(e);
    }

    let mut waiting = BTreeMap::new();
    let mut results = Vec::with_capacity(total);
    for msg in rx {
        if let Msg::Done(i, r) = msg {
            waiting.insert(i, r);
            while let Some(r) = waiting.remove(&results.len()) {
                on_result(&r);
                results.push(r);
            }
        }
    }
    for t in threads {
        let _ = t.join();
    }
    Ok(results)
}

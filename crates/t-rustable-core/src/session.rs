//! A session: one open document, its transactions, undo, the journal, and save.
//!
//! The engine always holds the state "original plus the first `applied` transactions".
//! The `IdMap` describes that same state. Any failed apply rolls back from bytes: the
//! nearest checkpoint or the original, plus a replay (ADR 0015).

use crate::assets::{AssetId, Assets};
use crate::command::{Command, PageReader};
use crate::coords::PageView;
use crate::error::{CoreError, Result, code};
use crate::ids::{AnnotationEntry, IdMap, ObjectId, PageContent, PageId, Seed};
use crate::journal::{self, Entry, Journal, Manifest};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use t_rustable_engine::{
    AnnotationInfo, Applied, Bitmap, Clip, DocKey, DocSummary, EditOp, Engine, EngineError,
    FormField, Metadata, ObjectInfo, OutlineItem, PageInfo, Priority, Query, Request, Response,
    SearchOptions, TextRun, Ticket,
};

/// A checkpoint every K transactions (ARCHITECTURE section 7).
pub const CHECKPOINT_EVERY: usize = 20;
/// The oldest checkpoints go first. The original is always kept.
pub const MAX_CHECKPOINTS: usize = 8;

static NEXT_KEY: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq)]
pub struct Committed {
    pub id: u64,
    pub label: String,
    /// "user", or "mod:<mod id>".
    pub source: String,
    pub commands: Vec<Command>,
}

/// What a commit, undo, or redo changed. The UI and the mods get this as `document-changed`.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub transaction: Option<u64>,
    pub affected_pages: Vec<PageId>,
    pub structure_changed: bool,
}

/// An open transaction. Commands are checked when they are added.
pub struct Transaction {
    label: String,
    source: String,
    commands: Vec<Command>,
    state: IdMap,
}

impl Transaction {
    pub fn commands(&self) -> &[Command] {
        &self.commands
    }

    /// The pages after the commands so far.
    pub fn page_ids(&self) -> Vec<PageId> {
        self.state.page_ids()
    }

    /// The page of an object in the state after the commands so far.
    pub fn locate(&self, object: ObjectId) -> Result<PageId> {
        crate::command::locate(&self.state, object)
    }
}

struct Checkpoint {
    applied: usize,
    path: PathBuf,
    ids: IdMap,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SaveOptions {
    /// Save also when this breaks the digital signatures of the document.
    pub allow_invalidate_signatures: bool,
}

pub struct OpenOptions<'a> {
    /// Where the journal folder goes.
    pub sessions_dir: &'a Path,
    pub password: Option<&'a str>,
    /// File name of the original, for the recover list.
    pub source_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PageDetails {
    pub id: PageId,
    pub label: String,
    pub info: PageInfo,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Identified<T> {
    pub id: ObjectId,
    pub info: T,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FieldDetails {
    pub id: ObjectId,
    pub page: PageId,
    pub field: FormField,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub page: PageId,
    pub text: String,
    pub boxes: Vec<t_rustable_engine::Rect>,
}

/// One bookmark of the document outline.
#[derive(Clone, Debug, PartialEq)]
pub struct OutlineEntry {
    pub title: String,
    /// None if the bookmark has no destination in this document.
    pub page: Option<PageId>,
    pub children: Vec<OutlineEntry>,
}

/// A render in the engine queue. It does not hold the session, so another thread can
/// wait for it.
pub struct RenderTicket {
    ticket: Ticket,
}

impl RenderTicket {
    /// Cancels the render. `wait` then gives `cancelled`.
    pub fn cancel(&self) {
        self.ticket.cancel();
    }

    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        self.ticket.cancel_flag()
    }

    /// Waits for the bitmap. If the engine restarted or lost the document, this gives
    /// `engine-restarted`. Then call `Session::submit_render` again.
    pub fn wait(self) -> Result<Bitmap> {
        match self.ticket.wait() {
            Ok(Response::Rendered(b)) => Ok(b),
            Ok(r) => Err(unexpected(r)),
            Err(EngineError::Restarted | EngineError::UnknownDocument) => Err(CoreError::new(
                code::ENGINE_RESTARTED,
                "The engine restarted before the render was done.",
            )
            .with_hint("Submit the render again.")),
            Err(e) => Err(e.into()),
        }
    }
}

/// Reads pages of the live document and keeps their IDs in the live `IdMap`.
struct LiveReader<'a> {
    engine: &'a Engine,
    key: DocKey,
    seed: &'a Seed,
    live: &'a mut IdMap,
}

fn ask(engine: &Engine, priority: Priority, req: Request) -> Result<Response> {
    engine.call(priority, req).map_err(CoreError::from)
}

fn unexpected(r: Response) -> CoreError {
    CoreError::internal(format!("unexpected engine reply: {r:?}"))
}

impl PageReader for LiveReader<'_> {
    fn read(&mut self, page: PageId) -> Result<(u8, PageContent)> {
        // A page that an earlier command of the open transaction copied from another PDF.
        let index = self.live.page_index(page).ok_or_else(|| {
            CoreError::invalid(format!(
                "Page {page} is new in this transaction. Commit the transaction, then change the page."
            ))
        })?;
        if let (Some(r), Some(c)) = (
            self.live.pages()[index].rotation,
            &self.live.pages()[index].content,
        ) {
            return Ok((r, c.clone()));
        }
        let query = |query| {
            ask(
                self.engine,
                Priority::Interactive,
                Request::Query {
                    doc: self.key,
                    query,
                },
            )
        };
        let Response::PageInfo(info) = query(Query::PageInfo { page: index })? else {
            return Err(CoreError::internal("no page info"));
        };
        let entry = &self.live.pages()[index];
        let content = match &entry.content {
            Some(c) => c.clone(),
            None => {
                let objects = match query(Query::Objects { page: index })? {
                    Response::Objects(o) => o.len(),
                    r => return Err(unexpected(r)),
                };
                let annotations = match query(Query::Annotations { page: index })? {
                    Response::Annotations(a) => a,
                    r => return Err(unexpected(r)),
                };
                PageContent {
                    objects: (0..objects).map(|i| self.seed.object(page, i)).collect(),
                    annotations: annotations
                        .iter()
                        .enumerate()
                        .map(|(i, a)| AnnotationEntry {
                            id: self.seed.annotation(page, i),
                            widget: a.subtype == "Widget",
                        })
                        .collect(),
                }
            }
        };
        let rotation = entry.rotation.unwrap_or(info.quarter_turns);
        let entry = self.live.entry_mut(index);
        entry.rotation = Some(rotation);
        entry.content = Some(content.clone());
        Ok((rotation, content))
    }
}

pub struct Session {
    engine: Engine,
    key: DocKey,
    restarts_seen: u64,
    original: Arc<[u8]>,
    password: Option<String>,
    seed: Seed,
    ids: IdMap,
    assets: Assets,
    history: Vec<Committed>,
    applied: usize,
    next_tx: u64,
    checkpoints: Vec<Checkpoint>,
    signature_count: usize,
    encrypted: bool,
    journal: Journal,
}

/// Opens a document only to read its summary, then closes it. No journal.
pub fn probe(engine: &Engine, bytes: Arc<[u8]>, password: Option<&str>) -> Result<DocSummary> {
    let key = NEXT_KEY.fetch_add(1, Ordering::Relaxed);
    let req = Request::Open {
        doc: key,
        bytes,
        password: password.map(String::from),
    };
    let summary = match ask(engine, Priority::Interactive, req)? {
        Response::Opened(s) => s,
        r => return Err(unexpected(r)),
    };
    let _ = engine.call(Priority::Interactive, Request::Close { doc: key });
    Ok(summary)
}

impl Session {
    /// Opens a document and starts its journal.
    pub fn open(engine: &Engine, bytes: Arc<[u8]>, options: OpenOptions<'_>) -> Result<Self> {
        let seed = Seed::random();
        let manifest = Manifest {
            format: journal::FORMAT,
            app_version: crate::VERSION.into(),
            pdfium: journal::PDFIUM_RELEASE.into(),
            seed,
            original_sha256: AssetId::of(&bytes),
            source_name: options.source_name,
        };
        Self::start(
            engine,
            bytes,
            options.password,
            seed,
            Assets::default(),
            |original| Journal::create(options.sessions_dir, &manifest, original),
        )
    }

    /// Opens a session again from its journal, after a crash.
    pub fn recover(engine: &Engine, dir: &Path, password: Option<&str>) -> Result<Self> {
        let r = Journal::read(dir)?;
        // Checkpoints of the crashed run are not valid for this run.
        for item in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let name = item.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("checkpoint-") && name.ends_with(".pdf") {
                let _ = std::fs::remove_file(item.path());
            }
        }
        let assets = r.assets.clone();
        let mut s = Self::start(
            engine,
            r.original,
            password,
            r.manifest.seed,
            r.assets,
            |_| Journal::reopen(dir, &assets),
        )?;
        let mut applied = 0;
        for entry in r.entries {
            match entry {
                Entry::Commit {
                    id,
                    label,
                    source,
                    commands,
                } => {
                    s.history.truncate(applied);
                    s.history.push(Committed {
                        id,
                        label,
                        source,
                        commands,
                    });
                    applied += 1;
                    s.next_tx = s.next_tx.max(id + 1);
                }
                Entry::Undo => {
                    applied = applied.checked_sub(1).ok_or_else(|| {
                        CoreError::new(
                            code::REPLAY_FAILED,
                            "The journal has an undo with nothing to undo.",
                        )
                    })?
                }
                Entry::Redo if applied < s.history.len() => applied += 1,
                Entry::Redo => {
                    return Err(CoreError::new(
                        code::REPLAY_FAILED,
                        "The journal has a redo with nothing to redo.",
                    ));
                }
            }
        }
        s.replay_to(applied).map_err(|e| {
            e.with_hint(format!(
                "The journal was made with PDFium {}. This app uses {}.",
                r.manifest.pdfium,
                journal::PDFIUM_RELEASE
            ))
        })?;
        Ok(s)
    }

    fn start(
        engine: &Engine,
        bytes: Arc<[u8]>,
        password: Option<&str>,
        seed: Seed,
        assets: Assets,
        journal: impl FnOnce(&[u8]) -> Result<Journal>,
    ) -> Result<Self> {
        let key = NEXT_KEY.fetch_add(1, Ordering::Relaxed);
        let req = Request::Open {
            doc: key,
            bytes: bytes.clone(),
            password: password.map(String::from),
        };
        let summary = match ask(engine, Priority::Interactive, req)? {
            Response::Opened(s) => s,
            r => return Err(unexpected(r)),
        };
        let journal = match journal(&bytes) {
            Ok(j) => j,
            Err(e) => {
                let _ = engine.call(Priority::Interactive, Request::Close { doc: key });
                return Err(e);
            }
        };
        Ok(Self {
            engine: engine.clone(),
            key,
            restarts_seen: engine.restarts(),
            original: bytes,
            password: password.map(String::from),
            seed,
            ids: IdMap::original(&seed, summary.page_count),
            assets,
            history: Vec::new(),
            applied: 0,
            next_tx: 1,
            checkpoints: Vec::new(),
            signature_count: summary.signature_count,
            encrypted: summary.encrypted,
            journal,
        })
    }

    /// Closes the session and deletes its journal and checkpoints.
    pub fn close(self) -> Result<()> {
        let _ = self
            .engine
            .call(Priority::Interactive, Request::Close { doc: self.key });
        self.journal.delete()
    }

    pub fn journal_dir(&self) -> &Path {
        self.journal.dir()
    }

    pub fn signature_count(&self) -> usize {
        self.signature_count
    }

    /// The file has a user or owner password. A save keeps the encryption.
    pub fn encrypted(&self) -> bool {
        self.encrypted
    }

    pub fn history(&self) -> &[Committed] {
        &self.history[..self.applied]
    }

    pub fn can_undo(&self) -> bool {
        self.applied > 0
    }

    pub fn can_redo(&self) -> bool {
        self.applied < self.history.len()
    }

    /// Adds an image or font that a command can use.
    pub fn add_asset(&mut self, data: Arc<[u8]>) -> AssetId {
        self.assets.add(data)
    }

    // Engine access ------------------------------------------------------------------

    /// After an engine restart, all documents are closed. Reload this one.
    fn ensure_open(&mut self) -> Result<()> {
        let restarts = self.engine.restarts();
        if restarts != self.restarts_seen {
            self.restarts_seen = restarts;
            self.replay_to(self.applied)?;
        }
        Ok(())
    }

    fn request(&mut self, priority: Priority, req: Request) -> Result<Response> {
        self.ensure_open()?;
        match self.engine.call(priority, req.clone()) {
            Err(EngineError::Restarted | EngineError::UnknownDocument) => {
                self.restarts_seen = self.engine.restarts();
                self.replay_to(self.applied)?;
                ask(&self.engine, priority, req)
            }
            r => r.map_err(CoreError::from),
        }
    }

    fn query(&mut self, query: Query) -> Result<Response> {
        self.request(
            Priority::Normal,
            Request::Query {
                doc: self.key,
                query,
            },
        )
    }

    fn reader(&mut self) -> LiveReader<'_> {
        LiveReader {
            engine: &self.engine,
            key: self.key,
            seed: &self.seed,
            live: &mut self.ids,
        }
    }

    fn page_index(&self, page: PageId) -> Result<usize> {
        self.ids
            .page_index(page)
            .ok_or_else(|| CoreError::not_found(format!("Page {page}")))
    }

    /// Reads the content IDs of a page.
    fn content(&mut self, page: PageId) -> Result<PageContent> {
        self.ensure_open()?;
        self.reader().read(page).map(|(_, c)| c)
    }

    // Transactions -------------------------------------------------------------------

    pub fn begin(&self, label: &str, source: &str) -> Transaction {
        Transaction {
            label: label.into(),
            source: source.into(),
            commands: Vec::new(),
            state: self.ids.clone(),
        }
    }

    /// Checks a command against the state after the commands before it, then adds it.
    pub fn add(&mut self, tx: &mut Transaction, mut command: Command) -> Result<()> {
        self.ensure_open()?;
        let mut reader = LiveReader {
            engine: &self.engine,
            key: self.key,
            seed: &self.seed,
            live: &mut self.ids,
        };
        command.resolve(&mut tx.state, &mut reader);
        command
            .step(&mut tx.state, &self.assets, &mut reader)
            .map_err(|e| e.at_command(tx.commands.len()))?;
        tx.commands.push(command);
        Ok(())
    }

    /// Checks all commands again and applies them at once. On error, nothing changes.
    pub fn commit(&mut self, tx: Transaction) -> Result<Change> {
        self.ensure_open()?;
        let committed = Committed {
            id: self.next_tx,
            label: tx.label,
            source: tx.source,
            commands: tx.commands,
        };
        self.apply(&committed.commands)?;
        if let Err(e) = self.journal.append(
            &Entry::Commit {
                id: committed.id,
                label: committed.label.clone(),
                source: committed.source.clone(),
                commands: committed.commands.clone(),
            },
            &self.assets,
        ) {
            // Live state and journal must match. Undo the apply.
            self.replay_to(self.applied)?;
            return Err(e);
        }
        self.next_tx += 1;
        self.history.truncate(self.applied);
        let applied = self.applied;
        self.checkpoints.retain(|c| {
            let keep = c.applied <= applied;
            if !keep {
                let _ = std::fs::remove_file(&c.path);
            }
            keep
        });
        self.history.push(committed);
        self.applied += 1;
        self.maybe_checkpoint();
        Ok(change(&self.history[self.applied - 1]))
    }

    /// Applies commands to the live document as one engine request. On any error, the
    /// session rolls back from bytes to the state before.
    fn apply(&mut self, commands: &[Command]) -> Result<()> {
        let mut state = self.ids.clone();
        let mut ops: Vec<EditOp> = Vec::new();
        let mut expect = Vec::new();
        let mut owner = Vec::new();
        {
            let mut reader = LiveReader {
                engine: &self.engine,
                key: self.key,
                seed: &self.seed,
                live: &mut self.ids,
            };
            for (k, c) in commands.iter().enumerate() {
                for (op, index) in c
                    .step(&mut state, &self.assets, &mut reader)
                    .map_err(|e| e.at_command(k))?
                {
                    ops.push(op);
                    expect.push(index);
                    owner.push(k);
                }
            }
        }
        if ops.is_empty() {
            self.ids = state;
            return Ok(());
        }
        let result = match ask_raw(&self.engine, Request::Apply { doc: self.key, ops }) {
            Ok(Response::Applied(applied)) => {
                let drift = applied.iter().zip(&expect).position(|(a, e)| match e {
                    Some(index) => *a != Applied::Object { index: *index },
                    None => false,
                });
                match drift {
                    None if applied.len() == expect.len() => Ok(()),
                    _ => Err(CoreError::internal(
                        "the engine object indices differ from the ID map",
                    )),
                }
            }
            Ok(r) => Err(unexpected(r)),
            Err(EngineError::OpFailed { index, error }) => {
                let k = owner[index];
                let e = CoreError::from(*error);
                let e = if e.code == code::INVALID_ARGUMENT || e.code == code::INTERNAL {
                    CoreError::new(code::COMMAND_FAILED, e.message)
                        .with_details(e.details.unwrap_or_default())
                } else {
                    e
                };
                Err(e.at_command(k))
            }
            Err(e) => Err(CoreError::from(e)),
        };
        match result {
            Ok(()) => {
                self.ids = state;
                Ok(())
            }
            Err(e) => {
                self.restarts_seen = self.engine.restarts();
                self.replay_to(self.applied)
                    .map_err(|r| r.with_details(format!("rollback after: {e}")))?;
                Err(e)
            }
        }
    }

    /// Opens the nearest checkpoint at or before `n` (or the original) and replays the
    /// transactions up to `n`. Sets `applied` to `n`.
    fn replay_to(&mut self, n: usize) -> Result<()> {
        let cp = self.checkpoints.iter().rev().find(|c| c.applied <= n);
        let (start, bytes, ids): (usize, Arc<[u8]>, IdMap) = match cp {
            Some(c) => {
                let bytes =
                    std::fs::read(&c.path).map_err(|e| CoreError::io("read checkpoint", e))?;
                (c.applied, bytes.into(), c.ids.clone())
            }
            None => (0, self.original.clone(), IdMap::original(&self.seed, 0)),
        };
        let req = Request::Open {
            doc: self.key,
            bytes,
            password: self.password.clone(),
        };
        let summary = match ask(&self.engine, Priority::Interactive, req)? {
            Response::Opened(s) => s,
            r => return Err(unexpected(r)),
        };
        self.ids = if start == 0 {
            IdMap::original(&self.seed, summary.page_count)
        } else {
            ids
        };
        self.applied = start;
        for i in start..n {
            let commands = self.history[i].commands.clone();
            self.apply_replay(&commands).map_err(|e| {
                CoreError::new(
                    code::REPLAY_FAILED,
                    format!(
                        "Transaction {} cannot be applied again: {}",
                        i + 1,
                        e.message
                    ),
                )
                .with_details(e.to_string())
            })?;
            self.applied = i + 1;
        }
        Ok(())
    }

    /// Like `apply`, but without a rollback: the caller handles a failure.
    fn apply_replay(&mut self, commands: &[Command]) -> Result<()> {
        let mut state = self.ids.clone();
        let mut ops = Vec::new();
        {
            let mut reader = LiveReader {
                engine: &self.engine,
                key: self.key,
                seed: &self.seed,
                live: &mut self.ids,
            };
            for (k, c) in commands.iter().enumerate() {
                ops.extend(
                    c.step(&mut state, &self.assets, &mut reader)
                        .map_err(|e| e.at_command(k))?
                        .into_iter()
                        .map(|(op, _)| op),
                );
            }
        }
        if !ops.is_empty() {
            ask(
                &self.engine,
                Priority::Interactive,
                Request::Apply { doc: self.key, ops },
            )?;
        }
        self.ids = state;
        Ok(())
    }

    fn maybe_checkpoint(&mut self) {
        if !self.applied.is_multiple_of(CHECKPOINT_EVERY)
            || self.checkpoints.iter().any(|c| c.applied == self.applied)
        {
            return;
        }
        // A failed checkpoint only makes undo slower.
        let bytes = match ask(
            &self.engine,
            Priority::Background,
            Request::Save { doc: self.key },
        ) {
            Ok(Response::Saved(bytes)) => bytes,
            Ok(r) => return log::warn!("No checkpoint: {}", unexpected(r)),
            Err(e) => return log::warn!("No checkpoint: {e}"),
        };
        let path = self
            .journal
            .dir()
            .join(format!("checkpoint-{}.pdf", self.applied));
        if let Err(e) = std::fs::write(&path, &bytes) {
            let _ = std::fs::remove_file(&path);
            return log::warn!("No checkpoint: {}: {e}", path.display());
        }
        self.checkpoints.push(Checkpoint {
            applied: self.applied,
            path,
            ids: self.ids.clone(),
        });
        if self.checkpoints.len() > MAX_CHECKPOINTS {
            let old = self.checkpoints.remove(0);
            let _ = std::fs::remove_file(old.path);
        }
    }

    pub fn undo(&mut self) -> Result<Change> {
        if self.applied == 0 {
            return Err(CoreError::new(
                code::NOTHING_TO_UNDO,
                "There is nothing to undo.",
            ));
        }
        self.ensure_open()?;
        let target = self.applied - 1;
        self.replay_to(target)?;
        if let Err(e) = self.journal.append(&Entry::Undo, &self.assets) {
            self.replay_to(target + 1)?;
            return Err(e);
        }
        Ok(change(&self.history[target]))
    }

    pub fn redo(&mut self) -> Result<Change> {
        if self.applied == self.history.len() {
            return Err(CoreError::new(
                code::NOTHING_TO_REDO,
                "There is nothing to redo.",
            ));
        }
        self.ensure_open()?;
        let commands = self.history[self.applied].commands.clone();
        self.apply(&commands)?;
        if let Err(e) = self.journal.append(&Entry::Redo, &self.assets) {
            self.replay_to(self.applied)?;
            return Err(e);
        }
        self.applied += 1;
        Ok(change(&self.history[self.applied - 1]))
    }

    // Queries ------------------------------------------------------------------------

    pub fn page_ids(&self) -> Vec<PageId> {
        self.ids.page_ids()
    }

    pub fn page(&mut self, page: PageId) -> Result<PageDetails> {
        let index = self.page_index(page)?;
        let Response::PageInfo(info) = self.query(Query::PageInfo { page: index })? else {
            return Err(CoreError::internal("no page info"));
        };
        let label = info
            .label
            .clone()
            .unwrap_or_else(|| (index + 1).to_string());
        Ok(PageDetails {
            id: page,
            label,
            info,
        })
    }

    pub fn page_view(&mut self, page: PageId, scale: f64) -> Result<PageView> {
        let d = self.page(page)?;
        Ok(PageView {
            crop_box: d.info.crop_box,
            quarter_turns: d.info.quarter_turns,
            scale,
        })
    }

    pub fn objects(&mut self, page: PageId) -> Result<Vec<Identified<ObjectInfo>>> {
        let index = self.page_index(page)?;
        let content = self.content(page)?;
        let Response::Objects(objects) = self.query(Query::Objects { page: index })? else {
            return Err(CoreError::internal("no objects"));
        };
        zip_ids(&content.objects, objects)
    }

    pub fn annotations(&mut self, page: PageId) -> Result<Vec<Identified<AnnotationInfo>>> {
        let index = self.page_index(page)?;
        let content = self.content(page)?;
        let Response::Annotations(annots) = self.query(Query::Annotations { page: index })? else {
            return Err(CoreError::internal("no annotations"));
        };
        let ids: Vec<ObjectId> = content.annotations.iter().map(|a| a.id).collect();
        zip_ids(&ids, annots)
    }

    /// Text runs. `object` of a run is the ID of its text object.
    pub fn text_runs(&mut self, page: PageId) -> Result<Vec<(Option<ObjectId>, TextRun)>> {
        let index = self.page_index(page)?;
        let content = self.content(page)?;
        let Response::TextRuns(runs) = self.query(Query::TextRuns { page: index })? else {
            return Err(CoreError::internal("no text runs"));
        };
        Ok(runs
            .into_iter()
            .map(|r| (r.object.and_then(|i| content.objects.get(i).copied()), r))
            .collect())
    }

    pub fn page_text(&mut self, page: PageId) -> Result<String> {
        let index = self.page_index(page)?;
        match self.query(Query::PageText { page: index })? {
            Response::PageText(t) => Ok(t),
            r => Err(unexpected(r)),
        }
    }

    pub fn form_fields(&mut self) -> Result<Vec<FieldDetails>> {
        let Response::FormFields(fields) = self.query(Query::FormFields)? else {
            return Err(CoreError::internal("no form fields"));
        };
        let mut out = Vec::with_capacity(fields.len());
        for field in fields {
            let page = self
                .ids
                .pages()
                .get(field.page)
                .map(|p| p.id)
                .ok_or_else(|| CoreError::internal("bad field page"))?;
            let content = self.content(page)?;
            let id = content
                .annotations
                .get(field.annotation)
                .map(|a| a.id)
                .ok_or_else(|| CoreError::internal("bad field annotation"))?;
            out.push(FieldDetails { id, page, field });
        }
        Ok(out)
    }

    pub fn search(&mut self, query: &str, options: SearchOptions) -> Result<Vec<Hit>> {
        let Response::SearchHits(hits) = self.query(Query::Search {
            query: query.into(),
            options,
        })?
        else {
            return Err(CoreError::internal("no search hits"));
        };
        hits.into_iter()
            .map(|h| {
                let page = self
                    .ids
                    .pages()
                    .get(h.page)
                    .map(|p| p.id)
                    .ok_or_else(|| CoreError::internal("bad hit page"))?;
                Ok(Hit {
                    page,
                    text: h.text,
                    boxes: h.boxes,
                })
            })
            .collect()
    }

    /// The bookmarks, with the pages as IDs.
    pub fn outline(&mut self) -> Result<Vec<OutlineEntry>> {
        let Response::Outline(items) = self.query(Query::Outline)? else {
            return Err(CoreError::internal("no outline"));
        };
        Ok(outline_entries(&self.ids, items))
    }

    pub fn metadata(&mut self) -> Result<Metadata> {
        match self.query(Query::Summary)? {
            Response::Summary(s) => Ok(s.metadata),
            r => Err(unexpected(r)),
        }
    }

    /// Puts a render in the engine queue and returns at once. Reloads the document first
    /// if the engine restarted.
    pub fn submit_render(
        &mut self,
        page: PageId,
        scale: f64,
        clip: Option<Clip>,
        priority: Priority,
    ) -> Result<RenderTicket> {
        self.ensure_open()?;
        let index = self.page_index(page)?;
        let ticket = self.engine.submit(
            priority,
            Request::Render {
                doc: self.key,
                page: index,
                scale,
                clip,
            },
        );
        Ok(RenderTicket { ticket })
    }

    pub fn render(
        &mut self,
        page: PageId,
        scale: f64,
        clip: Option<Clip>,
        priority: Priority,
    ) -> Result<Bitmap> {
        match self.submit_render(page, scale, clip, priority)?.wait() {
            Err(e) if e.code == code::ENGINE_RESTARTED => {
                self.restarts_seen = self.engine.restarts();
                self.replay_to(self.applied)?;
                self.submit_render(page, scale, clip, priority)?.wait()
            }
            r => r,
        }
    }

    // Save ---------------------------------------------------------------------------

    /// PDFium save, qpdf post-process, temporary file, flush, rename.
    pub fn save(&mut self, target: &Path, options: SaveOptions) -> Result<()> {
        if self.signature_count > 0 && !options.allow_invalidate_signatures {
            return Err(CoreError::new(
                code::SIGNED_DOCUMENT,
                "The document has digital signatures. A save breaks them.",
            )
            .with_hint("Confirm that the save can break the signatures, or save a copy."));
        }
        let bytes = match self.request(Priority::Interactive, Request::Save { doc: self.key })? {
            Response::Saved(b) => b,
            r => return Err(unexpected(r)),
        };
        let out = t_rustable_engine::post::post_process(&bytes).map_err(|e| {
            CoreError::internal("The qpdf step of the save failed.").with_details(e)
        })?;
        crate::save::write_atomic(target, &out)
    }
}

fn ask_raw(engine: &Engine, req: Request) -> std::result::Result<Response, EngineError> {
    engine.call(Priority::Interactive, req)
}

fn zip_ids<T>(ids: &[ObjectId], infos: Vec<T>) -> Result<Vec<Identified<T>>> {
    if ids.len() != infos.len() {
        return Err(CoreError::internal(format!(
            "the ID map has {} entries, but the engine has {}",
            ids.len(),
            infos.len()
        )));
    }
    Ok(ids
        .iter()
        .zip(infos)
        .map(|(id, info)| Identified { id: *id, info })
        .collect())
}

fn outline_entries(ids: &IdMap, items: Vec<OutlineItem>) -> Vec<OutlineEntry> {
    items
        .into_iter()
        .map(|i| OutlineEntry {
            title: i.title,
            page: i.page.and_then(|p| ids.pages().get(p)).map(|p| p.id),
            children: outline_entries(ids, i.children),
        })
        .collect()
}

fn change(tx: &Committed) -> Change {
    let mut pages = BTreeSet::new();
    let mut structure = false;
    for c in &tx.commands {
        let (page, s) = c.affected();
        pages.extend(page);
        structure |= s;
    }
    Change {
        transaction: Some(tx.id),
        affected_pages: pages.into_iter().collect(),
        structure_changed: structure,
    }
}

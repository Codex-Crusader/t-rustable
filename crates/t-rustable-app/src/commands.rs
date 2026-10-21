//! Tauri commands: requests from the UI (ARCHITECTURE section 11).

use crate::bridge::{GuiBridge, ModUi};
use crate::modthread::ModThread;
use crate::state::{DocDto, DocId, Shared};
use crate::wire::{self, Box2, KeyIn, PointerIn, ShapeDto};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use t_rustable_core::{Change, CoreError, OpenOptions, PageId, SaveOptions, Session};
use t_rustable_engine::{Point, SearchOptions};
use t_rustable_plugin_host::{HostError, Outcome};
use tauri::{Emitter, State};
use tauri_plugin_dialog::DialogExt;

pub struct App {
    pub shared: Arc<Shared>,
    pub bridge: Arc<GuiBridge>,
    pub mods: ModThread,
    /// Cancels the running mod action.
    pub action_cancel: Mutex<Option<Arc<AtomicBool>>>,
    pub self_test: Option<PathBuf>,
    /// Files from the command line, to open at start.
    pub files: Vec<PathBuf>,
    /// `.tmod` packages from the command line, to offer for install at start.
    pub packages: Vec<PathBuf>,
    /// Held while the app runs. None if another instance has it.
    pub instance_lock: Option<std::fs::File>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorDto {
    pub code: String,
    pub message: String,
    pub hint: Option<String>,
}

impl ErrorDto {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            hint: None,
        }
    }
}

impl From<CoreError> for ErrorDto {
    fn from(e: CoreError) -> Self {
        Self {
            code: e.code.into(),
            message: e.message,
            hint: e.hint,
        }
    }
}

impl From<HostError> for ErrorDto {
    fn from(e: HostError) -> Self {
        Self::new(e.code, e.message)
    }
}

impl From<String> for ErrorDto {
    fn from(message: String) -> Self {
        Self::new("internal", message)
    }
}

pub type Reply<T> = Result<T, ErrorDto>;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DocChanged {
    doc: DocId,
    pages: Vec<PageId>,
    structure: bool,
}

impl App {
    fn session(&self, doc: DocId) -> Reply<t_rustable_plugin_host::SessionRef> {
        self.shared
            .session(doc)
            .ok_or_else(|| ErrorDto::new("not-found", "The document is not open."))
    }

    fn emit_changed(&self, doc: DocId, changes: &[Change]) {
        if changes.is_empty() {
            return;
        }
        if let Err(e) = self.shared.apply_changes(doc, changes) {
            self.shared.log.write("error", "app", &e.to_string());
        }
        if let Some(app) = self.shared.app.get() {
            let _ = app.emit(
                "document-changed",
                DocChanged {
                    doc,
                    pages: changes
                        .iter()
                        .flat_map(|c| c.affected_pages.iter().copied())
                        .collect(),
                    structure: changes.iter().any(|c| c.structure_changed),
                },
            );
        }
    }

    /// Updates the UI and tells the interested mods about changes. Commits that the
    /// mods make in `on-document-changed` update the UI only, so this cannot loop.
    async fn changed(&self, doc: DocId, changes: Vec<Change>) {
        if changes.is_empty() {
            return;
        }
        self.emit_changed(doc, &changes);
        let Some(session) = self.shared.session(doc) else {
            return;
        };
        let mut listeners: Vec<String> = self
            .shared
            .open_panels
            .lock()
            .unwrap()
            .iter()
            .map(|(m, _)| m.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        if let Some(tool) = self.shared.active_tool.lock().unwrap().clone()
            && !listeners.contains(&tool)
        {
            listeners.push(tool);
        }
        if listeners.is_empty() {
            return;
        }
        let more = self
            .mods
            .call(move |m| {
                let mut more = Vec::new();
                for id in listeners {
                    for c in &changes {
                        match m.host.on_document_changed(&id, &session, c) {
                            Ok(out) => more.extend(out.changes),
                            Err(e) => more.extend(e.changes),
                        }
                    }
                }
                more
            })
            .await
            .unwrap_or_default();
        self.emit_changed(doc, &more);
    }

    /// Applies the commits of a mod call and turns the result into a reply.
    async fn finish<T>(&self, doc: Option<DocId>, out: Result<Outcome<T>, HostError>) -> Reply<T> {
        let doc = doc.or_else(|| self.shared.active_doc());
        match out {
            Ok(out) => {
                if let Some(doc) = doc {
                    self.changed(doc, out.changes).await;
                }
                Ok(out.value)
            }
            Err(mut e) => {
                if let Some(doc) = doc {
                    self.changed(doc, std::mem::take(&mut e.changes)).await;
                }
                Err(e.into())
            }
        }
    }
}

fn api_error(e: t_rustable_plugin_host::api::types::ApiError) -> ErrorDto {
    ErrorDto {
        code: e.code,
        message: e.message,
        hint: e.hint,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: &'static str,
    platform: &'static str,
    self_test: Option<String>,
    files: Vec<String>,
    packages: Vec<String>,
}

#[tauri::command]
pub fn app_info(app: State<'_, App>) -> AppInfo {
    AppInfo {
        version: t_rustable_core::VERSION,
        platform: if cfg!(windows) { "windows" } else { "linux" },
        self_test: app.self_test.as_ref().map(|p| p.display().to_string()),
        files: app.files.iter().map(|p| p.display().to_string()).collect(),
        packages: app
            .packages
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
    }
}

/// Opens a PDF. Without a path, the user picks one in a dialog. Returns None if the
/// user cancels.
#[tauri::command]
pub async fn open_file(
    app: State<'_, App>,
    path: Option<String>,
    password: Option<String>,
) -> Reply<Option<DocDto>> {
    let path = match path {
        Some(p) => PathBuf::from(p),
        None => {
            let Some(handle) = app.shared.app.get().cloned() else {
                return Ok(None);
            };
            let picked = tauri::async_runtime::spawn_blocking(move || {
                handle
                    .dialog()
                    .file()
                    .add_filter("PDF", &["pdf"])
                    .blocking_pick_file()
            })
            .await
            .map_err(|e| e.to_string())?;
            match picked.and_then(|p| p.into_path().ok()) {
                Some(p) => p,
                None => return Ok(None),
            }
        }
    };
    let shared = app.shared.clone();
    let doc = tauri::async_runtime::spawn_blocking(move || -> Reply<DocId> {
        let bytes = std::fs::read(&path)
            .map_err(|e| ErrorDto::new("io-error", format!("{}: {e}", path.display())))?;
        let title = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Document".into());
        let session = Session::open(
            &shared.engine,
            bytes.into(),
            OpenOptions {
                sessions_dir: &shared.folders.sessions,
                password: password.as_deref(),
                source_name: Some(title.clone()),
            },
        )?;
        Ok(shared.add_doc(session, Some(path), title)?)
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(app.shared.doc_dto(doc))
}

#[tauri::command]
pub fn document(app: State<'_, App>, doc: DocId) -> Reply<DocDto> {
    app.shared
        .doc_dto(doc)
        .ok_or_else(|| ErrorDto::new("not-found", "The document is not open."))
}

/// Closes a document. A document with unsaved changes needs `force`.
#[tauri::command]
pub async fn close_doc(app: State<'_, App>, doc: DocId, force: bool) -> Reply<()> {
    let removed = {
        let mut docs = app.shared.docs.lock().unwrap();
        match docs.get(&doc) {
            Some(d) if d.dirty && !force => {
                return Err(ErrorDto::new(
                    "unsaved-changes",
                    "The document has unsaved changes.",
                ));
            }
            Some(_) => docs.remove(&doc),
            None => None,
        }
    };
    app.shared.tiles.forget_doc(doc);
    if let Some(d) = removed
        && let Ok(m) = Arc::try_unwrap(d.session)
    {
        let session = m.into_inner().unwrap();
        tauri::async_runtime::spawn_blocking(move || session.close())
            .await
            .map_err(|e| e.to_string())??;
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextIn {
    doc: Option<DocId>,
    active_page: Option<PageId>,
    selected_pages: Vec<PageId>,
    zoom: f64,
}

#[tauri::command]
pub fn set_context(app: State<'_, App>, context: ContextIn) {
    *app.shared.context.lock().unwrap() = crate::state::Context {
        doc: context.doc,
        active_page: context.active_page,
        selected_pages: context.selected_pages,
        zoom: context.zoom,
    };
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextRunDto {
    text: String,
    #[serde(rename = "box")]
    bounds: Box2,
    size: f64,
    vertical: bool,
}

/// The text of a page for the text layer, in display points.
#[tauri::command]
pub async fn page_text(app: State<'_, App>, doc: DocId, page: PageId) -> Reply<Vec<TextRunDto>> {
    let session = app.session(doc)?;
    let view = app
        .shared
        .view(doc, page)
        .ok_or_else(|| ErrorDto::new("not-found", "The page is not in the document."))?;
    tauri::async_runtime::spawn_blocking(move || -> Reply<Vec<TextRunDto>> {
        let runs = session.lock().unwrap().text_runs(page)?;
        Ok(runs
            .into_iter()
            .filter(|(_, r)| !r.text.trim().is_empty())
            .map(|(_, r)| {
                let bounds = wire::display_box(&view, r.bounds);
                let vertical = view.quarter_turns % 2 == 1;
                TextRunDto {
                    text: r.text,
                    bounds,
                    size: r.font_size,
                    vertical,
                }
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HitDto {
    page: PageId,
    text: String,
    boxes: Vec<Box2>,
}

#[tauri::command]
pub async fn search(
    app: State<'_, App>,
    doc: DocId,
    query: String,
    match_case: bool,
    whole_word: bool,
) -> Reply<Vec<HitDto>> {
    let session = app.session(doc)?;
    let shared = app.shared.clone();
    tauri::async_runtime::spawn_blocking(move || -> Reply<Vec<HitDto>> {
        let hits = session.lock().unwrap().search(
            &query,
            SearchOptions {
                match_case,
                whole_word,
                max_hits: 1000,
            },
        )?;
        Ok(hits
            .into_iter()
            .filter_map(|h| {
                let view = shared.view(doc, h.page)?;
                Some(HitDto {
                    page: h.page,
                    text: h.text,
                    boxes: h
                        .boxes
                        .iter()
                        .map(|&b| wire::display_box(&view, b))
                        .collect(),
                })
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn registry(app: State<'_, App>) -> Reply<Vec<ModUi>> {
    let disabled: Vec<String> = app.mods.call(|m| m.stopped()).await?;
    let mut mods: Vec<ModUi> = app.bridge.mods.lock().unwrap().values().cloned().collect();
    for m in &mut mods {
        m.disabled = disabled.contains(&m.id);
    }
    Ok(mods)
}

/// Makes `id` the active tool, or no tool. Deactivates the old tool.
async fn switch_tool(app: &App, id: Option<String>) -> Reply<()> {
    let old = app.shared.active_tool.lock().unwrap().clone();
    if old == id {
        return Ok(());
    }
    if let Some(old) = old {
        let id = old.clone();
        let out = app.mods.call(move |m| m.host.deactivate(&id)).await?;
        let _ = app.finish(None, out).await;
        app.bridge.clear_overlay_of(&old);
    }
    *app.shared.active_tool.lock().unwrap() = id.clone();
    if let Some(id) = id {
        let out = app.mods.call(move |m| m.host.activate(&id)).await?;
        app.finish(None, out).await?.map_err(api_error)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn select_tool(app: State<'_, App>, tool: Option<String>) -> Reply<()> {
    switch_tool(&app, tool).await
}

/// Runs a mod action on the active document. A tool action first makes its mod the
/// active tool.
#[tauri::command]
pub async fn run_action(
    app: State<'_, App>,
    mod_id: String,
    action_id: String,
    args: Option<String>,
) -> Reply<()> {
    let is_tool = app
        .bridge
        .mods
        .lock()
        .unwrap()
        .get(&mod_id)
        .and_then(|m| m.actions.iter().find(|a| a.id == action_id).map(|a| a.tool))
        .ok_or_else(|| ErrorDto::new("not-found", "The action does not exist."))?;
    if is_tool {
        switch_tool(&app, Some(mod_id.clone())).await?;
    }
    let doc = app.shared.active_doc();
    let session = doc.and_then(|d| app.shared.session(d));
    let cancel = Arc::new(AtomicBool::new(false));
    *app.action_cancel.lock().unwrap() = Some(cancel.clone());
    let args = args.unwrap_or_else(|| "{}".into());
    let out = app
        .mods
        .call(move |m| {
            m.host
                .run_action(&mod_id, session.as_ref(), &action_id, &args, cancel)
        })
        .await?;
    *app.action_cancel.lock().unwrap() = None;
    app.finish(doc, out).await?.map_err(api_error)
}

#[tauri::command]
pub fn cancel_action(app: State<'_, App>) {
    if let Some(flag) = app.action_cancel.lock().unwrap().as_ref() {
        flag.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PointerReply {
    cursor: &'static str,
    handled: bool,
    overlay: Vec<ShapeDto>,
}

#[tauri::command]
pub async fn pointer(app: State<'_, App>, doc: DocId, event: PointerIn) -> Reply<PointerReply> {
    let Some(tool) = app.shared.active_tool.lock().unwrap().clone() else {
        return Ok(PointerReply {
            cursor: "default",
            handled: false,
            overlay: Vec::new(),
        });
    };
    let session = app.session(doc)?;
    let view = app
        .shared
        .view(doc, event.page)
        .ok_or_else(|| ErrorDto::new("not-found", "The page is not in the document."))?;
    let event = wire::pointer(&view, &event).map_err(|m| ErrorDto::new("invalid-argument", m))?;
    let out = app
        .mods
        .call(move |m| m.host.on_pointer(&tool, &session, event))
        .await?;
    let result = app.finish(Some(doc), out).await?;
    Ok(PointerReply {
        cursor: wire::cursor(result.cursor),
        handled: result.handled,
        overlay: app.bridge.convert(&result.overlay),
    })
}

#[tauri::command]
pub async fn key(app: State<'_, App>, doc: DocId, event: KeyIn) -> Reply<bool> {
    let Some(tool) = app.shared.active_tool.lock().unwrap().clone() else {
        return Ok(false);
    };
    let session = app.session(doc)?;
    let event = wire::key(&event);
    let out = app
        .mods
        .call(move |m| m.host.on_key(&tool, &session, event))
        .await?;
    Ok(app.finish(Some(doc), out).await?.handled)
}

#[tauri::command]
pub async fn text_input(app: State<'_, App>, doc: DocId, text: String) -> Reply<()> {
    let Some(tool) = app.shared.active_tool.lock().unwrap().clone() else {
        return Ok(());
    };
    let session = app.session(doc)?;
    let out = app
        .mods
        .call(move |m| m.host.on_text_input(&tool, &session, &text))
        .await?;
    app.finish(Some(doc), out).await
}

#[tauri::command]
pub fn answer_confirm(app: State<'_, App>, id: u64, yes: bool) {
    app.bridge.answer_confirm(id, yes);
}

#[tauri::command]
pub async fn undo(app: State<'_, App>, doc: DocId, redo: bool) -> Reply<()> {
    let session = app.session(doc)?;
    let change = tauri::async_runtime::spawn_blocking(move || {
        let mut s = session.lock().unwrap();
        if redo { s.redo() } else { s.undo() }
    })
    .await
    .map_err(|e| e.to_string())??;
    app.changed(doc, vec![change]).await;
    Ok(())
}

/// Saves a document. `save_as`, or a document without a path, shows a save dialog.
/// Returns None if the user cancels.
#[tauri::command]
pub async fn save(
    app: State<'_, App>,
    doc: DocId,
    save_as: bool,
    allow_invalidate_signatures: bool,
    target: Option<String>,
) -> Reply<Option<DocDto>> {
    let session = app.session(doc)?;
    let (current, title) = {
        let docs = app.shared.docs.lock().unwrap();
        let d = docs
            .get(&doc)
            .ok_or_else(|| ErrorDto::new("not-found", "The document is not open."))?;
        (d.path.clone(), d.title.clone())
    };
    let target = match (target, current) {
        // The self-test saves into the log folder, never next to user files.
        (Some(t), _) if app.self_test.is_some() => app.shared.folders.log.join(t),
        (_, Some(p)) if !save_as => p,
        _ => {
            let Some(handle) = app.shared.app.get().cloned() else {
                return Ok(None);
            };
            let picked = tauri::async_runtime::spawn_blocking(move || {
                handle
                    .dialog()
                    .file()
                    .add_filter("PDF", &["pdf"])
                    .set_file_name(&title)
                    .blocking_save_file()
            })
            .await
            .map_err(|e| e.to_string())?;
            match picked.and_then(|p| p.into_path().ok()) {
                Some(p) => p,
                None => return Ok(None),
            }
        }
    };
    let path = target.clone();
    tauri::async_runtime::spawn_blocking(move || {
        session.lock().unwrap().save(
            &path,
            SaveOptions {
                allow_invalidate_signatures,
            },
        )
    })
    .await
    .map_err(|e| e.to_string())??;
    if let Some(d) = app.shared.docs.lock().unwrap().get_mut(&doc) {
        d.dirty = false;
        d.title = target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or(d.title.clone());
        d.path = Some(target);
    }
    Ok(app.shared.doc_dto(doc))
}

#[tauri::command]
pub fn cancel_tiles(app: State<'_, App>, paths: Vec<String>) {
    app.shared.cancel_tiles(&paths);
}

/// A point on a page, from display points to PDF coordinates. For mod action arguments
/// that the UI fills in, such as the position of `add-text`.
#[tauri::command]
pub fn to_pdf(app: State<'_, App>, doc: DocId, page: PageId, x: f64, y: f64) -> Reply<[f64; 2]> {
    let view = app
        .shared
        .view(doc, page)
        .ok_or_else(|| ErrorDto::new("not-found", "The page is not in the document."))?;
    let p = view.from_display(Point { x, y });
    Ok([p.x, p.y])
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlineDto {
    title: String,
    page: Option<PageId>,
    children: Vec<OutlineDto>,
}

fn outline_dto(entries: Vec<t_rustable_core::session::OutlineEntry>) -> Vec<OutlineDto> {
    entries
        .into_iter()
        .map(|e| OutlineDto {
            title: e.title,
            page: e.page,
            children: outline_dto(e.children),
        })
        .collect()
}

#[tauri::command]
pub async fn outline(app: State<'_, App>, doc: DocId) -> Reply<Vec<OutlineDto>> {
    let session = app.session(doc)?;
    tauri::async_runtime::spawn_blocking(move || -> Reply<Vec<OutlineDto>> {
        Ok(outline_dto(session.lock().unwrap().outline()?))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The UI opened or closed a panel. Mods with an open panel get document changes.
#[tauri::command]
pub fn panel_open(app: State<'_, App>, mod_id: String, panel_id: String, open: bool) {
    let mut panels = app.shared.open_panels.lock().unwrap();
    if open {
        panels.insert((mod_id, panel_id));
    } else {
        panels.remove(&(mod_id, panel_id));
    }
}

/// The largest panel message, in bytes.
const MAX_PANEL_MESSAGE: usize = 1 << 20;

/// A message from a panel to its mod. The UI sends only messages from the iframe of
/// that panel.
#[tauri::command]
pub async fn panel_message(
    app: State<'_, App>,
    mod_id: String,
    panel_id: String,
    message: String,
) -> Reply<()> {
    if message.len() > MAX_PANEL_MESSAGE {
        return Err(ErrorDto::new(
            "invalid-argument",
            "The panel message is too large.",
        ));
    }
    let key = (mod_id.clone(), panel_id.clone());
    if !app.shared.open_panels.lock().unwrap().contains(&key) {
        return Err(ErrorDto::new("not-found", "The panel is not open."));
    }
    let out = app
        .mods
        .call(move |m| m.host.on_panel_message(&mod_id, &panel_id, &message))
        .await?;
    app.finish(None, out).await?.map_err(api_error)
}

/// The `--tr-*` variables for the panel theme file.
#[tauri::command]
pub fn set_panel_theme(app: State<'_, App>, css: String) -> Reply<()> {
    if css.len() > crate::panels::MAX_THEME {
        return Err(ErrorDto::new("invalid-argument", "The theme is too large."));
    }
    *app.bridge.theme_css.lock().unwrap() = css;
    Ok(())
}

/// The largest keymap.json that the app reads.
const MAX_KEYMAP: u64 = 256 * 1024;

/// The text of the user keymap.json, or None if there is no file. The UI parses it.
#[tauri::command]
pub fn read_keymap(app: State<'_, App>) -> Reply<Option<String>> {
    let path = app.shared.folders.config.join("keymap.json");
    let Ok(meta) = std::fs::metadata(&path) else {
        return Ok(None);
    };
    if meta.len() > MAX_KEYMAP {
        return Err(ErrorDto::new("file-too-large", "keymap.json is too large."));
    }
    std::fs::read_to_string(&path)
        .map(Some)
        .map_err(|e| ErrorDto::new("io-error", format!("keymap.json cannot be read: {e}")))
}

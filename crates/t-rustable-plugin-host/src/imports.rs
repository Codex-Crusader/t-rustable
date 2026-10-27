//! The host side of the API 0.1 imports.

use crate::bindings::v0_1::generated::t_rustable::api::{document, edit, files, host, types, ui};
use crate::bindings::v0_1::{self as cv, api_error};
use crate::services::{FileFilter, MAX_TEXT, mod_target, printable};
use crate::state::{
    CallWorld, DocRes, HostState, MAX_OPEN_TRANSACTIONS, MAX_OVERLAY_IMAGE, MAX_PICKED_FILE,
    MAX_SETTINGS, PickedRes, SessionRef, TxRes,
};
use std::sync::atomic::Ordering;
use t_rustable_core::assets::sniff_image;
use t_rustable_core::command::Command;
use t_rustable_core::{CoreError, ObjectId, PageId, Session, Transaction};
use types::ApiError;
use wasmtime::component::Resource;

fn expired() -> ApiError {
    api_error(
        "expired",
        "The handle belongs to an earlier call. Handles are valid only during the call that made them.",
    )
}

fn lock(session: &SessionRef) -> std::sync::MutexGuard<'_, Session> {
    session.lock().unwrap_or_else(|e| e.into_inner())
}

impl HostState {
    fn doc_session(&self, doc: &Resource<DocRes>) -> Result<SessionRef, ApiError> {
        self.table
            .get(doc)
            .map(|d| d.session.clone())
            .map_err(|_| api_error("internal", "The document handle is not valid."))
    }

    fn with_doc<T>(
        &mut self,
        doc: &Resource<DocRes>,
        f: impl FnOnce(&mut Session) -> Result<T, CoreError>,
    ) -> Result<T, ApiError> {
        let session = self.doc_session(doc)?;
        let mut s = lock(&session);
        f(&mut s).map_err(cv::error)
    }

    /// Runs `f` with the open transaction and its session.
    fn with_tx<T>(
        &mut self,
        res: &Resource<TxRes>,
        f: impl FnOnce(&mut Session, &mut Transaction) -> Result<T, CoreError>,
    ) -> Result<T, ApiError> {
        let generation = self.generation;
        let entry = self.table.get_mut(res).map_err(|_| expired())?;
        if entry.generation != generation {
            return Err(expired());
        }
        let mut tx = entry.tx.take().ok_or_else(expired)?;
        let session = entry.session.clone();
        let result = f(&mut lock(&session), &mut tx);
        if let Ok(entry) = self.table.get_mut(res) {
            entry.tx = Some(tx);
        }
        result.map_err(cv::error)
    }

    fn add(
        &mut self,
        res: &Resource<TxRes>,
        make: impl FnOnce(&mut Session, &Transaction) -> Result<Command, CoreError>,
    ) -> Result<(), ApiError> {
        self.with_tx(res, |s, tx| {
            let command = make(s, tx)?;
            s.add(tx, command)
        })
    }

    fn world(&self) -> CallWorld {
        self.call.as_ref().map_or(CallWorld::Batch, |c| c.world)
    }

    fn display_scale(&self, page: types::PageId) -> Result<f64, ApiError> {
        let no_display = || api_error("no-display", "There is no page view for this call.");
        if self.world() == CallWorld::Batch {
            return Err(no_display());
        }
        self.services.ui.display_scale(page).ok_or_else(no_display)
    }
}

impl types::Host for HostState {}

impl document::Host for HostState {
    fn measure_text(&mut self, text: String, style: types::TextStyle) -> Result<f64, ApiError> {
        let font = self
            .services
            .fonts
            .select(&style.font_family, style.bold, style.italic)
            .map_err(cv::error)?;
        t_rustable_core::fonts::text_width(&font, &text, style.size).map_err(cv::error)
    }
}

impl document::HostDoc for HostState {
    fn page_ids(&mut self, doc: Resource<DocRes>) -> Vec<types::PageId> {
        self.doc_session(&doc)
            .map(|s| lock(&s).page_ids().into_iter().map(cv::api_page).collect())
            .unwrap_or_default()
    }

    fn page_info(
        &mut self,
        doc: Resource<DocRes>,
        page: types::PageId,
    ) -> Result<document::PageInfo, ApiError> {
        let d = self.with_doc(&doc, |s| s.page(cv::page_id(page)))?;
        let crop = d.info.crop_box;
        Ok(document::PageInfo {
            id: page,
            label: d.label,
            media_box: cv::api_rect(d.info.media_box),
            crop_box: cv::api_rect(crop),
            width: crop.width(),
            height: crop.height(),
            rotation: d.info.quarter_turns as u32 * 90,
        })
    }

    fn text_runs(
        &mut self,
        doc: Resource<DocRes>,
        page: types::PageId,
    ) -> Result<Vec<document::TextRun>, ApiError> {
        let runs = self.with_doc(&doc, |s| s.text_runs(cv::page_id(page)))?;
        Ok(runs
            .into_iter()
            .map(|(object, r)| document::TextRun {
                object: object.map(cv::api_object),
                text: r.text,
                font_name: r.font_name,
                font_size: r.font_size,
                bounds: cv::api_rect(r.bounds),
                chars: r
                    .chars
                    .into_iter()
                    .map(|c| document::CharInfo {
                        c: c.c,
                        bounds: cv::api_rect(c.bounds),
                    })
                    .collect(),
            })
            .collect())
    }

    fn objects(
        &mut self,
        doc: Resource<DocRes>,
        page: types::PageId,
    ) -> Result<Vec<document::ObjectInfo>, ApiError> {
        let objects = self.with_doc(&doc, |s| s.objects(cv::page_id(page)))?;
        Ok(objects
            .into_iter()
            .map(|o| document::ObjectInfo {
                id: cv::api_object(o.id),
                kind: cv::object_kind(o.info.kind),
                bounds: cv::api_rect(o.info.bounds),
            })
            .collect())
    }

    fn annotations(
        &mut self,
        doc: Resource<DocRes>,
        page: types::PageId,
    ) -> Result<Vec<document::AnnotationInfo>, ApiError> {
        let annots = self.with_doc(&doc, |s| s.annotations(cv::page_id(page)))?;
        Ok(annots
            .into_iter()
            .map(|a| document::AnnotationInfo {
                id: cv::api_object(a.id),
                subtype: a.info.subtype,
                bounds: cv::api_rect(a.info.bounds),
                contents: a.info.contents,
            })
            .collect())
    }

    fn form_fields(
        &mut self,
        doc: Resource<DocRes>,
    ) -> Result<Vec<document::FormFieldInfo>, ApiError> {
        let fields = self.with_doc(&doc, |s| s.form_fields())?;
        Ok(fields
            .into_iter()
            .map(|f| document::FormFieldInfo {
                id: cv::api_object(f.id),
                page: cv::api_page(f.page),
                name: f.field.name,
                kind: cv::field_kind(f.field.kind),
                value: f.field.value,
                bounds: cv::api_rect(f.field.bounds),
                read_only: f.field.read_only,
            })
            .collect())
    }

    fn search(
        &mut self,
        doc: Resource<DocRes>,
        query: String,
        options: document::SearchOptions,
    ) -> Result<Vec<document::SearchHit>, ApiError> {
        let options = t_rustable_engine::SearchOptions {
            match_case: options.match_case,
            whole_word: options.whole_word,
            max_hits: options.max_hits.min(10_000) as usize,
        };
        let hits = self.with_doc(&doc, |s| s.search(&query, options))?;
        Ok(hits
            .into_iter()
            .map(|h| document::SearchHit {
                page: cv::api_page(h.page),
                text: h.text,
                boxes: h.boxes.into_iter().map(cv::api_rect).collect(),
            })
            .collect())
    }

    fn metadata(&mut self, doc: Resource<DocRes>) -> document::Metadata {
        let (m, pages) = self
            .with_doc(&doc, |s| Ok((s.metadata()?, s.page_ids().len())))
            .unwrap_or_else(|_| (Default::default(), 0));
        document::Metadata {
            title: m.title,
            author: m.author,
            subject: m.subject,
            keywords: m.keywords,
            creator: m.creator,
            producer: m.producer,
            page_count: pages as u32,
        }
    }

    fn to_display(
        &mut self,
        doc: Resource<DocRes>,
        page: types::PageId,
        p: types::Point,
    ) -> Result<document::DisplayPoint, ApiError> {
        let scale = self.display_scale(page)?;
        let view = self.with_doc(&doc, |s| s.page_view(cv::page_id(page), scale))?;
        let d = view.to_display(cv::point(p));
        Ok(document::DisplayPoint { x: d.x, y: d.y })
    }

    fn from_display(
        &mut self,
        doc: Resource<DocRes>,
        page: types::PageId,
        p: document::DisplayPoint,
    ) -> Result<types::Point, ApiError> {
        let scale = self.display_scale(page)?;
        let view = self.with_doc(&doc, |s| s.page_view(cv::page_id(page), scale))?;
        Ok(cv::api_point(
            view.from_display(t_rustable_engine::Point { x: p.x, y: p.y }),
        ))
    }

    fn drop(&mut self, rep: Resource<DocRes>) -> wasmtime::Result<()> {
        // The host deletes the document entry after the call.
        let _ = rep;
        Ok(())
    }
}

impl edit::Host for HostState {
    fn begin(&mut self, doc: Resource<DocRes>, label: String) -> Result<Resource<TxRes>, ApiError> {
        let session = self.doc_session(&doc)?;
        let open = self.call.as_ref().map_or(0, |c| c.transactions.len());
        if open >= MAX_OPEN_TRANSACTIONS {
            return Err(api_error(
                "limit-exceeded",
                format!("A call can open at most {MAX_OPEN_TRANSACTIONS} transactions."),
            ));
        }
        let label = if label.trim().is_empty() {
            self.mod_id.clone()
        } else {
            label
        };
        let tx = lock(&session).begin(&label, &format!("mod:{}", self.mod_id));
        let res = self
            .table
            .push(TxRes {
                generation: self.generation,
                session,
                tx: Some(tx),
            })
            .map_err(|e| api_error("internal", e.to_string()))?;
        if let Some(call) = &mut self.call {
            call.transactions.push(res.rep());
        }
        Ok(res)
    }
}

impl edit::HostTransaction for HostState {
    fn delete_page(&mut self, tx: Resource<TxRes>, page: types::PageId) -> Result<(), ApiError> {
        self.add(&tx, |_, _| {
            Ok(Command::DeletePage {
                page: cv::page_id(page),
            })
        })
    }

    fn move_page(
        &mut self,
        tx: Resource<TxRes>,
        page: types::PageId,
        before: Option<types::PageId>,
    ) -> Result<(), ApiError> {
        self.add(&tx, |_, _| {
            Ok(Command::MovePage {
                page: cv::page_id(page),
                before: before.map(cv::page_id),
            })
        })
    }

    fn rotate_page(
        &mut self,
        tx: Resource<TxRes>,
        page: types::PageId,
        degrees: i32,
    ) -> Result<(), ApiError> {
        self.add(&tx, |_, _| {
            Ok(Command::RotatePage {
                page: cv::page_id(page),
                degrees,
            })
        })
    }

    fn insert_blank_page(
        &mut self,
        tx: Resource<TxRes>,
        before: Option<types::PageId>,
        width: f64,
        height: f64,
    ) -> Result<types::PageId, ApiError> {
        let id = PageId::random();
        self.add(&tx, |_, _| {
            Ok(Command::InsertBlankPage {
                id,
                before: before.map(cv::page_id),
                width,
                height,
            })
        })?;
        Ok(cv::api_page(id))
    }

    fn add_text(
        &mut self,
        tx: Resource<TxRes>,
        page: types::PageId,
        at: types::Point,
        text: String,
        style: types::TextStyle,
    ) -> Result<types::ObjectId, ApiError> {
        let id = ObjectId::random();
        let fonts = self.services.fonts.clone();
        self.add(&tx, |s, _| {
            let font = s.add_asset(fonts.select(&style.font_family, style.bold, style.italic)?);
            Ok(Command::AddText {
                id,
                page: cv::page_id(page),
                at: cv::point(at),
                text,
                font,
                size: style.size,
                color: cv::color(style.color),
                turns: None,
            })
        })?;
        Ok(cv::api_object(id))
    }

    fn add_image(
        &mut self,
        tx: Resource<TxRes>,
        page: types::PageId,
        bounds: types::Rect,
        data: Vec<u8>,
    ) -> Result<types::ObjectId, ApiError> {
        let id = ObjectId::random();
        self.add(&tx, |s, _| {
            let image = s.add_asset(data.into());
            Ok(Command::AddImage {
                id,
                page: cv::page_id(page),
                bounds: cv::rect(bounds),
                image,
                turns: None,
            })
        })?;
        Ok(cv::api_object(id))
    }

    fn add_shape(
        &mut self,
        tx: Resource<TxRes>,
        page: types::PageId,
        shape: edit::Shape,
        stroke: Option<types::Stroke>,
        fill: Option<types::Color>,
    ) -> Result<types::ObjectId, ApiError> {
        let id = ObjectId::random();
        self.add(&tx, |_, _| {
            Ok(Command::AddShape {
                id,
                page: cv::page_id(page),
                shape: cv::shape(shape),
                stroke: stroke.map(cv::stroke),
                fill: fill.map(cv::color),
            })
        })?;
        Ok(cv::api_object(id))
    }

    fn remove_object(
        &mut self,
        tx: Resource<TxRes>,
        object: types::ObjectId,
    ) -> Result<(), ApiError> {
        let object = cv::object_id(object);
        self.add(&tx, |_, tx| {
            Ok(Command::RemoveObject {
                page: tx.locate(object)?,
                object,
            })
        })
    }

    fn commit(&mut self, res: Resource<TxRes>) -> Result<u64, ApiError> {
        let entry = self.table.delete(res).map_err(|_| expired())?;
        let tx = match entry.tx {
            Some(tx) if entry.generation == self.generation => tx,
            _ => return Err(expired()),
        };
        let change = lock(&entry.session).commit(tx).map_err(cv::error)?;
        let id = change.transaction.unwrap_or_default();
        if let Some(call) = &mut self.call {
            call.changes.push(change);
        }
        Ok(id)
    }

    fn drop(&mut self, rep: Resource<TxRes>) -> wasmtime::Result<()> {
        let _ = self.table.delete(rep);
        Ok(())
    }
}

impl host::Host for HostState {
    fn log(&mut self, level: types::LogLevel, message: String) {
        let level = match level {
            types::LogLevel::Debug => log::Level::Debug,
            types::LogLevel::Info => log::Level::Info,
            types::LogLevel::Warn => log::Level::Warn,
            types::LogLevel::Error => log::Level::Error,
        };
        log::log!(target: &mod_target(&self.mod_id), level, "{}", printable(&message, MAX_TEXT));
    }

    fn progress(&mut self, fraction: f32, message: String) {
        let fraction = if fraction.is_finite() {
            fraction.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let message = printable(&message, MAX_TEXT);
        self.services.ui.progress(&self.mod_id, fraction, &message);
    }

    fn is_cancelled(&mut self) -> bool {
        self.call
            .as_ref()
            .is_some_and(|c| c.cancel.load(Ordering::Relaxed))
    }

    fn setting_get(&mut self, key: String) -> Option<String> {
        self.services.settings.get(&self.mod_id, &key)
    }

    fn setting_set(&mut self, key: String, value: String) -> Result<(), ApiError> {
        let store = &self.services.settings;
        let old = store
            .get(&self.mod_id, &key)
            .map_or(0, |v| key.len() + v.len());
        if store.size(&self.mod_id) - old + key.len() + value.len() > MAX_SETTINGS {
            return Err(api_error(
                "limit-exceeded",
                "The settings of a mod can use at most 64 KB.",
            ));
        }
        store.set(&self.mod_id, &key, &value);
        Ok(())
    }

    fn setting_remove(&mut self, key: String) {
        self.services.settings.remove(&self.mod_id, &key);
    }
}

fn ui_result(r: Result<(), String>) -> Result<(), ApiError> {
    r.map_err(|m| api_error("invalid-argument", m))
}

impl ui::Host for HostState {
    fn register_action(&mut self, action: ui::Action) -> Result<(), ApiError> {
        ui_result(self.services.ui.register_action(&self.mod_id, action))
    }

    fn register_toolbar_item(&mut self, item: ui::ToolbarItem) -> Result<(), ApiError> {
        ui_result(self.services.ui.register_toolbar_item(&self.mod_id, item))
    }

    fn register_panel(&mut self, panel: ui::Panel) -> Result<(), ApiError> {
        ui_result(self.services.ui.register_panel(&self.mod_id, panel))
    }

    fn suggest_shortcut(&mut self, shortcut: ui::Shortcut) -> Result<(), ApiError> {
        ui_result(self.services.ui.suggest_shortcut(&self.mod_id, shortcut))
    }

    fn set_overlay(&mut self, shapes: Vec<types::OverlayShape>) {
        let shapes = labelled(&self.mod_id, shapes);
        self.services.ui.set_overlay(&self.mod_id, shapes);
    }

    fn clear_overlay(&mut self) {
        self.services.ui.clear_overlay(&self.mod_id);
    }

    fn register_overlay_image(&mut self, data: Vec<u8>) -> Result<u32, ApiError> {
        if data.len() > MAX_OVERLAY_IMAGE {
            return Err(api_error(
                "limit-exceeded",
                "An overlay image can have at most 16 MB.",
            ));
        }
        if sniff_image(&data).is_none() {
            return Err(api_error(
                "unsupported-format",
                "The image is not a PNG or JPEG file.",
            ));
        }
        self.services
            .ui
            .register_overlay_image(&self.mod_id, data)
            .map_err(|m| api_error("unsupported-format", m))
    }

    fn post_to_panel(&mut self, panel_id: String, message: String) -> Result<(), ApiError> {
        ui_result(
            self.services
                .ui
                .post_to_panel(&self.mod_id, &panel_id, &message),
        )
    }

    fn notify(&mut self, level: ui::NotifyLevel, message: String) {
        let message = printable(&message, MAX_TEXT);
        self.services.ui.notify(&self.mod_id, level, &message);
    }

    fn confirm(&mut self, title: String, message: String) -> bool {
        let title = printable(&title, MAX_TEXT);
        self.services.ui.confirm(&self.mod_id, &title, &message)
    }

    fn context(&mut self) -> ui::UiContext {
        self.services.ui.context(&self.mod_id)
    }
}

/// Removes overlay shapes without a label (mod system rule 7), and logs a warning.
pub(crate) fn labelled(mod_id: &str, shapes: Vec<types::OverlayShape>) -> Vec<types::OverlayShape> {
    let total = shapes.len();
    let kept: Vec<_> = shapes
        .into_iter()
        .filter(|s| !label(s).trim().is_empty())
        .collect();
    if kept.len() < total {
        log::warn!(
            target: &mod_target(mod_id),
            "{} overlay shapes have no label. The host does not show them.",
            total - kept.len()
        );
    }
    kept
}

fn label(s: &types::OverlayShape) -> &str {
    use types::OverlayShape as S;
    match s {
        S::Rect(r) | S::Ellipse(r) => &r.label,
        S::Line(l) => &l.label,
        S::Polyline(p) => &p.label,
        S::Text(t) => &t.label,
        S::Image(i) => &i.label,
        S::Caret(c) => &c.label,
        S::Handle(h) => &h.label,
    }
}

impl files::Host for HostState {
    fn pick(
        &mut self,
        options: files::PickOptions,
    ) -> Result<Option<Resource<PickedRes>>, ApiError> {
        let filters: Vec<FileFilter> = options
            .filters
            .into_iter()
            .map(|f| FileFilter {
                name: f.name,
                extensions: f.extensions,
            })
            .collect();
        let Some(path) =
            self.services
                .files
                .pick(&self.mod_id, &printable(&options.title, MAX_TEXT), &filters)
        else {
            return Ok(None);
        };
        let size = std::fs::metadata(&path)
            .map_err(|e| api_error("internal", format!("The picked file cannot be read: {e}")))?
            .len();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let res = self
            .table
            .push(PickedRes {
                generation: self.generation,
                name,
                size,
                path,
            })
            .map_err(|e| api_error("internal", e.to_string()))?;
        Ok(Some(res))
    }
}

impl files::HostPickedFile for HostState {
    fn name(&mut self, file: Resource<PickedRes>) -> String {
        self.table
            .get(&file)
            .map(|f| f.name.clone())
            .unwrap_or_default()
    }

    fn size(&mut self, file: Resource<PickedRes>) -> u64 {
        self.table.get(&file).map_or(0, |f| f.size)
    }

    fn read(&mut self, file: Resource<PickedRes>) -> Result<Vec<u8>, ApiError> {
        let f = self.table.get(&file).map_err(|_| expired())?;
        if f.generation != self.generation {
            return Err(expired());
        }
        if f.size > MAX_PICKED_FILE {
            return Err(api_error(
                "file-too-large",
                "The file is larger than 256 MB.",
            ));
        }
        std::fs::read(&f.path)
            .map_err(|e| api_error("internal", format!("The picked file cannot be read: {e}")))
    }

    fn drop(&mut self, rep: Resource<PickedRes>) -> wasmtime::Result<()> {
        let _ = self.table.delete(rep);
        Ok(())
    }
}

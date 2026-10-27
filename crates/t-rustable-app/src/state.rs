//! State that all threads share: open documents, page revisions, and the UI context.
//!
//! Lock order: never lock a session while you hold `Shared::docs`. Copy the `SessionRef`
//! out first. The mod thread can hold a session lock while it calls the bridge.

use crate::tiles::TileCache;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use t_rustable_core::coords::PageView;
use t_rustable_core::paths::Folders;
use t_rustable_core::{Change, PageId, Result as CoreResult, Session};
use t_rustable_engine::Engine;
use t_rustable_plugin_host::SessionRef;

pub type DocId = u64;

pub struct Doc {
    pub session: SessionRef,
    /// The file that the document came from, or the last save target.
    pub path: Option<PathBuf>,
    pub title: String,
    pub dirty: bool,
    pub order: Vec<PageId>,
    /// Page views at zoom 1, for coordinate conversion.
    pub views: HashMap<PageId, PageView>,
    pub labels: HashMap<PageId, String>,
    pub revisions: HashMap<PageId, u64>,
    /// Increments when pages are added, removed, or moved.
    pub structure: u64,
}

/// What the UI needs to lay out one page.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageDto {
    pub id: PageId,
    pub label: String,
    /// Display size at zoom 1, in points.
    pub width: f64,
    pub height: f64,
    pub revision: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocDto {
    pub id: DocId,
    pub title: String,
    pub path: Option<String>,
    pub dirty: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub structure: u64,
    pub pages: Vec<PageDto>,
}

/// The UI state that mods read through `ui.context`.
#[derive(Clone, Debug, Default)]
pub struct Context {
    pub doc: Option<DocId>,
    pub active_page: Option<PageId>,
    pub selected_pages: Vec<PageId>,
    pub zoom: f64,
}

pub struct Shared {
    pub engine: Engine,
    pub folders: Folders,
    pub docs: Mutex<BTreeMap<DocId, Doc>>,
    next_doc: Mutex<DocId>,
    pub context: Mutex<Context>,
    pub tiles: TileCache,
    /// Cancel flags of tile requests, by tile path.
    pub in_flight: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// The mod that gets pointer and key events.
    pub active_tool: Mutex<Option<String>>,
    /// Open panels as (mod ID, panel ID). Their mods also get `on-document-changed`.
    pub open_panels: Mutex<HashSet<(String, String)>>,
    pub app: OnceLock<tauri::AppHandle>,
}

impl Shared {
    pub fn new(engine: Engine, folders: Folders) -> Self {
        Self {
            engine,
            folders,
            docs: Mutex::new(BTreeMap::new()),
            next_doc: Mutex::new(1),
            context: Mutex::new(Context {
                zoom: 1.0,
                ..Context::default()
            }),
            tiles: TileCache::new(256 << 20),
            in_flight: Mutex::new(HashMap::new()),
            active_tool: Mutex::new(None),
            open_panels: Mutex::new(HashSet::new()),
            app: OnceLock::new(),
        }
    }

    /// Adds an open session as a new document.
    pub fn add_doc(
        &self,
        session: Session,
        path: Option<PathBuf>,
        title: String,
    ) -> CoreResult<DocId> {
        let session: SessionRef = Arc::new(Mutex::new(session));
        let (order, views, labels) = read_pages(&session, None)?;
        let mut next = self.next_doc.lock().unwrap();
        let id = *next;
        *next += 1;
        let revisions = order.iter().map(|&p| (p, 0)).collect();
        self.docs.lock().unwrap().insert(
            id,
            Doc {
                session,
                path,
                title,
                dirty: false,
                order,
                views,
                labels,
                revisions,
                structure: 0,
            },
        );
        Ok(id)
    }

    pub fn session(&self, doc: DocId) -> Option<SessionRef> {
        self.docs
            .lock()
            .unwrap()
            .get(&doc)
            .map(|d| d.session.clone())
    }

    /// The document that holds a page.
    pub fn doc_of(&self, page: PageId) -> Option<DocId> {
        let docs = self.docs.lock().unwrap();
        docs.iter()
            .find(|(_, d)| d.views.contains_key(&page))
            .map(|(&id, _)| id)
    }

    pub fn view(&self, doc: DocId, page: PageId) -> Option<PageView> {
        self.docs
            .lock()
            .unwrap()
            .get(&doc)?
            .views
            .get(&page)
            .copied()
    }

    pub fn active_doc(&self) -> Option<DocId> {
        self.context.lock().unwrap().doc
    }

    /// Updates revisions and views after changes. Returns false if the document is gone.
    pub fn apply_changes(&self, doc: DocId, changes: &[Change]) -> CoreResult<bool> {
        if changes.is_empty() {
            return Ok(true);
        }
        let Some(session) = self.session(doc) else {
            return Ok(false);
        };
        let structure = changes.iter().any(|c| c.structure_changed);
        let affected: HashSet<PageId> = changes
            .iter()
            .flat_map(|c| c.affected_pages.iter().copied())
            .collect();
        let refresh = if structure { None } else { Some(&affected) };
        let (order, views, labels) = read_pages(&session, refresh)?;
        let mut docs = self.docs.lock().unwrap();
        let Some(d) = docs.get_mut(&doc) else {
            return Ok(false);
        };
        d.dirty = true;
        if structure {
            d.structure += 1;
            d.order = order;
            d.views = views;
            d.labels = labels;
        } else {
            d.views.extend(views);
            d.labels.extend(labels);
        }
        for page in &affected {
            *d.revisions.entry(*page).or_insert(0) += 1;
        }
        let known: HashSet<PageId> = d.order.iter().copied().collect();
        d.revisions.retain(|p, _| known.contains(p));
        for p in &d.order {
            d.revisions.entry(*p).or_insert(0);
        }
        drop(docs);
        self.tiles.forget_pages(doc, &affected);
        Ok(true)
    }

    pub fn doc_dto(&self, doc: DocId) -> Option<DocDto> {
        let session = self.session(doc)?;
        let (can_undo, can_redo) = {
            let s = session.lock().unwrap();
            (s.can_undo(), s.can_redo())
        };
        let docs = self.docs.lock().unwrap();
        let d = docs.get(&doc)?;
        let pages = d
            .order
            .iter()
            .filter_map(|p| {
                let (width, height) = d.views.get(p)?.display_size();
                Some(PageDto {
                    id: *p,
                    label: d.labels.get(p).cloned().unwrap_or_default(),
                    width,
                    height,
                    revision: d.revisions.get(p).copied().unwrap_or(0),
                })
            })
            .collect();
        Some(DocDto {
            id: doc,
            title: d.title.clone(),
            path: d.path.as_ref().map(|p| p.display().to_string()),
            dirty: d.dirty,
            can_undo,
            can_redo,
            structure: d.structure,
            pages,
        })
    }

    pub fn cancel_tiles(&self, paths: &[String]) {
        let map = self.in_flight.lock().unwrap();
        for p in paths {
            if let Some(flag) = map.get(p) {
                flag.store(true, Ordering::Relaxed);
            }
        }
    }
}

type Pages = (
    Vec<PageId>,
    HashMap<PageId, PageView>,
    HashMap<PageId, String>,
);

/// Reads the page order, and the views and labels of `only` (all pages if None).
fn read_pages(session: &SessionRef, only: Option<&HashSet<PageId>>) -> CoreResult<Pages> {
    let mut s = session.lock().unwrap();
    let order = s.page_ids();
    let mut views = HashMap::new();
    let mut labels = HashMap::new();
    for (i, &p) in order.iter().enumerate() {
        if only.is_some_and(|set| !set.contains(&p)) {
            continue;
        }
        let d = s.page(p)?;
        views.insert(
            p,
            PageView {
                crop_box: d.info.crop_box,
                quarter_turns: d.info.quarter_turns,
                scale: 1.0,
            },
        );
        let label = if d.label.is_empty() {
            (i + 1).to_string()
        } else {
            d.label
        };
        labels.insert(p, label);
    }
    Ok((order, views, labels))
}

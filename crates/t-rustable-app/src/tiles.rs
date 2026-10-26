//! Tiles over the `trtile` URI scheme (ARCHITECTURE section 10 and 11).
//!
//! Path forms:
//! - `/<doc>/<page>/<revision>/<scale x 1000>/<tx>_<ty>.png`: one 512 px tile.
//! - `/image/<mod id>/<image id>`: an overlay image of a mod.

use crate::state::{DocId, Shared};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use t_rustable_core::PageId;
use t_rustable_core::error::code;
use t_rustable_engine::{Bitmap, Clip, Priority};

pub const TILE: u32 = 512;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TileKey {
    pub doc: DocId,
    pub page: PageId,
    pub revision: u64,
    pub scale_milli: u32,
    pub tx: u32,
    pub ty: u32,
}

impl TileKey {
    pub fn parse(path: &str) -> Option<Self> {
        let mut parts = path.trim_matches('/').split('/');
        let doc = parts.next()?.parse().ok()?;
        let page = parts.next()?.parse().ok()?;
        let revision = parts.next()?.parse().ok()?;
        let scale_milli: u32 = parts.next()?.parse().ok()?;
        let (tx, ty) = parts.next()?.strip_suffix(".png")?.split_once('_')?;
        if parts.next().is_some() || !(10..=64_000).contains(&scale_milli) {
            return None;
        }
        Some(Self {
            doc,
            page,
            revision,
            scale_milli,
            tx: tx.parse().ok()?,
            ty: ty.parse().ok()?,
        })
    }

    pub fn scale(&self) -> f64 {
        f64::from(self.scale_milli) / 1000.0
    }
}

/// An LRU cache of encoded tiles with a memory budget.
pub struct TileCache {
    inner: Mutex<CacheInner>,
}

struct CacheInner {
    budget: usize,
    used: usize,
    tick: u64,
    tiles: HashMap<TileKey, (Arc<Vec<u8>>, u64)>,
    order: BTreeMap<u64, TileKey>,
}

impl TileCache {
    pub fn new(budget: usize) -> Self {
        Self {
            inner: Mutex::new(CacheInner {
                budget,
                used: 0,
                tick: 0,
                tiles: HashMap::new(),
                order: BTreeMap::new(),
            }),
        }
    }

    pub fn get(&self, key: &TileKey) -> Option<Arc<Vec<u8>>> {
        let mut c = self.inner.lock().unwrap();
        c.tick += 1;
        let tick = c.tick;
        let (data, old) = c.tiles.get_mut(key).map(|e| {
            let old = e.1;
            e.1 = tick;
            (e.0.clone(), old)
        })?;
        c.order.remove(&old);
        c.order.insert(tick, key.clone());
        Some(data)
    }

    pub fn put(&self, key: TileKey, data: Arc<Vec<u8>>) {
        let mut c = self.inner.lock().unwrap();
        c.tick += 1;
        let tick = c.tick;
        c.used += data.len();
        if let Some((old, old_tick)) = c.tiles.insert(key.clone(), (data, tick)) {
            c.used -= old.len();
            c.order.remove(&old_tick);
        }
        c.order.insert(tick, key);
        while c.used > c.budget {
            let Some((_, oldest)) = c.order.pop_first() else {
                break;
            };
            if let Some((d, _)) = c.tiles.remove(&oldest) {
                c.used -= d.len();
            }
        }
    }

    /// Removes the tiles of changed pages.
    pub fn forget_pages(&self, doc: DocId, pages: &HashSet<PageId>) {
        self.retain(|k| k.doc != doc || !pages.contains(&k.page));
    }

    pub fn forget_doc(&self, doc: DocId) {
        self.retain(|k| k.doc != doc);
    }

    fn retain(&self, keep: impl Fn(&TileKey) -> bool) {
        let mut c = self.inner.lock().unwrap();
        let gone: Vec<TileKey> = c.tiles.keys().filter(|k| !keep(k)).cloned().collect();
        for k in gone {
            if let Some((d, tick)) = c.tiles.remove(&k) {
                c.used -= d.len();
                c.order.remove(&tick);
            }
        }
    }

    #[cfg(test)]
    pub fn used(&self) -> usize {
        self.inner.lock().unwrap().used
    }
}

pub enum TileError {
    BadPath,
    /// The page or its revision changed. The UI asks again with the new URL.
    Gone,
    Cancelled,
    Failed(String),
}

/// Renders one tile, or takes it from the cache.
pub fn tile(shared: &Shared, path: &str) -> Result<Arc<Vec<u8>>, TileError> {
    let key = TileKey::parse(path).ok_or(TileError::BadPath)?;
    if let Some(png) = shared.tiles.get(&key) {
        return Ok(png);
    }
    let (session, view, revision) = {
        let docs = shared.docs.lock().unwrap();
        let d = docs.get(&key.doc).ok_or(TileError::Gone)?;
        let view = *d.views.get(&key.page).ok_or(TileError::Gone)?;
        let revision = d.revisions.get(&key.page).copied().unwrap_or(0);
        (d.session.clone(), view, revision)
    };
    if revision != key.revision {
        return Err(TileError::Gone);
    }
    let (w, h) = view.display_size();
    let (pw, ph) = (
        (w * key.scale()).ceil() as u32,
        (h * key.scale()).ceil() as u32,
    );
    let (x, y) = (key.tx * TILE, key.ty * TILE);
    if x >= pw || y >= ph {
        return Err(TileError::BadPath);
    }
    let clip = Clip {
        x: x as i32,
        y: y as i32,
        width: TILE.min(pw - x),
        height: TILE.min(ph - y),
    };
    shared
        .in_flight
        .lock()
        .unwrap()
        .insert(path.to_string(), Arc::new(AtomicBool::new(false)));
    let result = render(shared, path, &session, key.page, key.scale(), clip);
    shared.in_flight.lock().unwrap().remove(path);
    let bitmap = result?;
    let png = Arc::new(bitmap.to_png());
    shared.tiles.put(key, png.clone());
    Ok(png)
}

fn render(
    shared: &Shared,
    path: &str,
    session: &t_rustable_plugin_host::SessionRef,
    page: PageId,
    scale: f64,
    clip: Clip,
) -> Result<Bitmap, TileError> {
    // The second try comes after an engine restart. `submit_render` replays first.
    for _ in 0..2 {
        let ticket = session
            .lock()
            .unwrap()
            .submit_render(page, scale, Some(clip), Priority::Interactive)
            .map_err(|e| TileError::Failed(e.to_string()))?;
        // From now on, `cancel_tiles` sets the flag that the engine reads.
        let flag = ticket.cancel_flag();
        let old = shared
            .in_flight
            .lock()
            .unwrap()
            .insert(path.to_string(), flag.clone());
        if old.is_some_and(|f| f.load(Ordering::Relaxed)) {
            flag.store(true, Ordering::Relaxed);
        }
        match ticket.wait() {
            Ok(b) => return Ok(b),
            Err(e) if e.code == code::ENGINE_RESTARTED => continue,
            Err(e) if e.code == code::CANCELLED => return Err(TileError::Cancelled),
            Err(e) => return Err(TileError::Failed(e.to_string())),
        }
    }
    Err(TileError::Failed("The engine restarted again.".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(n: u32) -> TileKey {
        TileKey {
            doc: 1,
            page: PageId(7),
            revision: 0,
            scale_milli: 1000,
            tx: n,
            ty: 0,
        }
    }

    #[test]
    fn paths_parse_and_bad_paths_fail() {
        let page = PageId(0xabc);
        let k = TileKey::parse(&format!("/3/{page}/2/1333/4_5.png")).unwrap();
        assert_eq!((k.doc, k.page, k.revision, k.tx, k.ty), (3, page, 2, 4, 5));
        assert!((k.scale() - 1.333).abs() < 1e-9);
        for bad in [
            "/3/x/2/1333/4_5.png",
            "/3",
            "/1/1/1/1000/1_1.jpg",
            "/1/1/1/0/1_1.png",
            "/1/1/1/1000/1_1.png/x",
        ] {
            assert!(TileKey::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn the_cache_drops_the_oldest_tile_first() {
        let cache = TileCache::new(250);
        for n in 0..3 {
            cache.put(key(n), Arc::new(vec![0; 100]));
        }
        assert!(cache.get(&key(0)).is_none());
        assert!(cache.get(&key(2)).is_some());
        cache.put(key(3), Arc::new(vec![0; 100]));
        // Tile 1 was used least recently, so it goes.
        assert!(cache.get(&key(1)).is_none());
        assert!(cache.get(&key(2)).is_some());
        assert_eq!(cache.used(), 200);
        cache.forget_pages(1, &HashSet::from([PageId(7)]));
        assert_eq!(cache.used(), 0);
    }
}

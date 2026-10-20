//! Recovery of sessions that did not close (ARCHITECTURE section 8), and the close of
//! all sessions when the app stops.

use crate::commands::{App, ErrorDto, Reply};
use crate::state::{DocDto, Shared};
use serde::Serialize;
use std::fs::File;
use std::path::{Path, PathBuf};
use t_rustable_core::Session;
use t_rustable_core::journal;
use tauri::State;

/// Takes the instance lock. Only the first running app gets it, and only that app
/// offers recovery. Another instance would see the live sessions of the first one.
pub fn instance_lock(state_dir: &Path) -> Option<File> {
    let file = File::create(state_dir.join("t-rustable.lock")).ok()?;
    file.try_lock().ok()?;
    Some(file)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnclosedDto {
    /// The name of the journal folder.
    id: String,
    title: String,
}

/// The journal folder of an ID from the UI. None if the ID is not one folder name.
fn journal_dir(shared: &Shared, id: &str) -> Option<PathBuf> {
    let ok = !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    let dir = shared.folders.sessions.join(id);
    (ok && dir.is_dir()).then_some(dir)
}

/// Journals that no open session uses. Empty if another app instance is running.
#[tauri::command]
pub fn unclosed(app: State<'_, App>) -> Reply<Vec<UnclosedDto>> {
    if app.instance_lock.is_none() {
        return Ok(Vec::new());
    }
    let open: Vec<PathBuf> = app
        .shared
        .docs
        .lock()
        .unwrap()
        .values()
        .map(|d| d.session.clone())
        .collect::<Vec<_>>()
        .into_iter()
        .map(|s| s.lock().unwrap().journal_dir().to_path_buf())
        .collect();
    let list = journal::list_unclosed(&app.shared.folders.sessions)?;
    Ok(list
        .into_iter()
        .filter(|u| !open.contains(&u.dir))
        .filter_map(|u| {
            Some(UnclosedDto {
                id: u.dir.file_name()?.to_str()?.to_string(),
                title: u.manifest.source_name.unwrap_or_else(|| "Untitled".into()),
            })
        })
        .collect())
}

/// Opens a session again from its journal. The document has no file, so the next
/// save asks for one, and it has unsaved changes.
#[tauri::command]
pub async fn recover(app: State<'_, App>, id: String, password: Option<String>) -> Reply<DocDto> {
    let shared = app.shared.clone();
    let dir = journal_dir(&shared, &id)
        .ok_or_else(|| ErrorDto::new("not-found", "The session does not exist."))?;
    let doc = tauri::async_runtime::spawn_blocking(move || -> Reply<_> {
        let title = journal::list_unclosed(&shared.folders.sessions)?
            .into_iter()
            .find(|u| u.dir == dir)
            .and_then(|u| u.manifest.source_name)
            .unwrap_or_else(|| "Untitled".into());
        let session = Session::recover(&shared.engine, &dir, password.as_deref())?;
        let doc = shared.add_doc(session, None, title)?;
        if let Some(d) = shared.docs.lock().unwrap().get_mut(&doc) {
            d.dirty = true;
        }
        Ok(doc)
    })
    .await
    .map_err(|e| e.to_string())??;
    app.shared
        .doc_dto(doc)
        .ok_or_else(|| ErrorDto::new("internal", "The recovered document is missing."))
}

/// Deletes a journal that the user does not want to recover.
#[tauri::command]
pub fn discard_unclosed(app: State<'_, App>, id: String) -> Reply<()> {
    let dir = journal_dir(&app.shared, &id)
        .ok_or_else(|| ErrorDto::new("not-found", "The session does not exist."))?;
    std::fs::remove_dir_all(&dir)
        .map_err(|e| ErrorDto::new("io-error", format!("{}: {e}", dir.display())))
}

/// Closes all sessions, which deletes their journals, and stops the app. The UI calls
/// this after the user answered the question about unsaved changes.
#[tauri::command]
pub async fn quit(app: State<'_, App>, handle: tauri::AppHandle) -> Reply<()> {
    let docs = std::mem::take(&mut *app.shared.docs.lock().unwrap());
    tauri::async_runtime::spawn_blocking(move || {
        for d in docs.into_values() {
            match std::sync::Arc::try_unwrap(d.session) {
                Ok(m) => drop(m.into_inner().unwrap().close()),
                // A render still holds the session. The journal goes anyway.
                Err(s) => drop(std::fs::remove_dir_all(s.lock().unwrap().journal_dir())),
            }
        }
    })
    .await
    .map_err(|e| e.to_string())?;
    handle.exit(0);
    Ok(())
}

//! Tauri commands of the Mods menu: install, the mod manager, and developer mode.

use crate::commands::{App, ErrorDto, Reply};
use crate::modreg::{ModRow, OfferDto};
use std::path::PathBuf;
use tauri::State;
use tauri_plugin_dialog::DialogExt;

/// A file or folder that the user picks. None if the user cancels.
async fn pick(app: &App, folder: bool) -> Reply<Option<PathBuf>> {
    let Some(handle) = app.shared.app.get().cloned() else {
        return Ok(None);
    };
    let picked = tauri::async_runtime::spawn_blocking(move || {
        let dialog = handle.dialog().file();
        if folder {
            dialog.blocking_pick_folder()
        } else {
            dialog
                .add_filter("T-Rustable mod", &["tmod"])
                .blocking_pick_file()
        }
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(picked.and_then(|p| p.into_path().ok()))
}

#[tauri::command]
pub async fn mod_list(app: State<'_, App>) -> Reply<Vec<ModRow>> {
    Ok(app.mods.call(|m| m.rows()).await?)
}

/// Reads and checks a `.tmod` package. Without a path, the user picks one. The
/// package waits for `install_answer`. Returns None if the user cancels.
#[tauri::command]
pub async fn install_offer(app: State<'_, App>, path: Option<String>) -> Reply<Option<OfferDto>> {
    let path = match path {
        Some(p) => PathBuf::from(p),
        None => match pick(&app, false).await? {
            Some(p) => p,
            None => return Ok(None),
        },
    };
    let bytes = tauri::async_runtime::spawn_blocking(move || -> Reply<Vec<u8>> {
        let io = |e: std::io::Error| ErrorDto::new("io-error", format!("{}: {e}", path.display()));
        let size = std::fs::metadata(&path).map_err(io)?.len();
        if size > t_rustable_tmod::LIMITS.package {
            return Err(ErrorDto::new(
                "package-too-large",
                format!(
                    "The package is larger than {} MiB.",
                    t_rustable_tmod::LIMITS.package >> 20
                ),
            ));
        }
        std::fs::read(&path).map_err(io)
    })
    .await
    .map_err(|e| e.to_string())??;
    let offer = app.mods.call(move |m| m.stage(&bytes)).await?;
    Ok(Some(offer?))
}

/// The user's answer to an install offer. Returns the mod ID after an install.
#[tauri::command]
pub async fn install_answer(
    app: State<'_, App>,
    token: u64,
    install: bool,
) -> Reply<Option<String>> {
    Ok(app.mods.call(move |m| m.answer(token, install)).await??)
}

#[tauri::command]
pub async fn mod_set_on(app: State<'_, App>, key: String, on: bool) -> Reply<()> {
    Ok(app.mods.call(move |m| m.set_on(&key, on)).await??)
}

#[tauri::command]
pub async fn mod_remove(app: State<'_, App>, id: String) -> Reply<()> {
    Ok(app.mods.call(move |m| m.remove(&id)).await??)
}

#[tauri::command]
pub async fn mod_reload(app: State<'_, App>, key: String) -> Reply<()> {
    Ok(app.mods.call(move |m| m.reload(&key)).await??)
}

#[tauri::command]
pub async fn developer_mode(app: State<'_, App>, on: Option<bool>) -> Reply<bool> {
    Ok(app
        .mods
        .call(move |m| match on {
            Some(on) => m.set_developer(on).map(|()| on),
            None => Ok(m.config.developer),
        })
        .await??)
}

/// Adds a developer folder. Without a path, the user picks one. Returns false if the
/// user cancels.
#[tauri::command]
pub async fn dev_folder_add(app: State<'_, App>, path: Option<String>) -> Reply<bool> {
    let dir = match path {
        Some(p) => PathBuf::from(p),
        None => match pick(&app, true).await? {
            Some(p) => p,
            None => return Ok(false),
        },
    };
    let dir = std::path::absolute(&dir).map_err(|e| e.to_string())?;
    app.mods.call(move |m| m.add_dev_folder(dir)).await??;
    Ok(true)
}

#[tauri::command]
pub async fn dev_folder_remove(app: State<'_, App>, path: String) -> Reply<()> {
    Ok(app
        .mods
        .call(move |m| m.remove_dev_folder(&PathBuf::from(path)))
        .await??)
}

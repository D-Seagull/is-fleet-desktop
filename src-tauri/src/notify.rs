//! Native notifications + downloads for the shell.
//!
//! The page is loaded from the web, so it can't attach a click handler to an
//! OS notification itself — and the notification plugin ignores clicks on
//! Windows. These Windows toasts carry an `on_activated` callback instead:
//! a click brings the window back and tells the page which notification it
//! was (`notification-clicked` event), so it can open that very chat.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Url};

/// Shown by the page for an incoming message (only when the window isn't
/// focused — the page decides). `id` comes back in `notification-clicked`.
#[tauri::command]
pub fn show_notification(
    app: AppHandle,
    id: Option<String>,
    title: String,
    body: String,
) -> Result<(), String> {
    let handle = app.clone();
    toast(&app, &title, &body, move || {
        crate::show_main(&handle);
        if let Some(id) = &id {
            let _ = handle.emit("notification-clicked", id.clone());
        }
    })
}

/// Payload of `download-started` / `download-finished` — the page shows a
/// "Downloading… → Saved" toast from these. `id` (the URL) pairs the two.
#[derive(Clone, Serialize)]
struct DownloadUpdate {
    id: String,
    name: String,
    path: Option<String>,
    success: bool,
}

/// A download starts: route it to Downloads (unique name) and tell the page.
pub fn download_requested(app: &AppHandle, url: &Url, destination: &mut PathBuf) {
    if let Ok(dir) = app.path().download_dir() {
        let name = destination
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "download".to_owned());
        *destination = unique_download_path(&dir, &name);
    }
    let _ = app.emit(
        "download-started",
        DownloadUpdate {
            id: url.to_string(),
            name: file_name_of(destination),
            path: None,
            success: false,
        },
    );
}

/// A download ended: tell the page (it shows "Saved" + "Show in folder").
/// If the window isn't in front, also a Windows toast so it isn't missed.
pub fn download_finished(app: &AppHandle, url: &Url, path: Option<PathBuf>, success: bool) {
    let name = path.as_deref().map(file_name_of).unwrap_or_default();
    let _ = app.emit(
        "download-finished",
        DownloadUpdate {
            id: url.to_string(),
            name,
            path: path.as_ref().map(|p| p.display().to_string()),
            success,
        },
    );
    let in_front = app
        .get_webview_window("main")
        .and_then(|w| w.is_focused().ok())
        .unwrap_or(false);
    if let (true, false, Some(path)) = (success, in_front, path) {
        notify_saved(app, &path);
    }
}

/// "Show in folder" from the page's toast. Only files inside Downloads —
/// the page is remote content and must not point Explorer anywhere else.
#[tauri::command]
pub fn reveal_download(app: AppHandle, path: String) -> Result<(), String> {
    let dir = app.path().download_dir().map_err(|e| e.to_string())?;
    let file = PathBuf::from(&path);
    let inside = file
        .canonicalize()
        .ok()
        .zip(dir.canonicalize().ok())
        .is_some_and(|(f, d)| f.starts_with(d));
    if !inside {
        return Err("not a downloaded file".into());
    }
    reveal_in_explorer(&file);
    Ok(())
}

fn file_name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// "File saved" after a download finished — click reveals it in Explorer.
pub fn notify_saved(app: &AppHandle, path: &Path) {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let reveal = path.to_path_buf();
    let _ = toast(app, &name, "Saved to Downloads", move || {
        reveal_in_explorer(&reveal);
    });
}

/// Where a download lands: Downloads\<name>, with " (n)" added when a file of
/// that name already exists (WebView2 would otherwise overwrite it).
pub fn unique_download_path(dir: &Path, file_name: &str) -> PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| file_name.to_owned());
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    (1..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .expect("an unused file name")
}

#[cfg(windows)]
fn toast<F>(app: &AppHandle, title: &str, body: &str, on_click: F) -> Result<(), String>
where
    F: Fn() + Send + 'static,
{
    use tauri_winrt_notification::Toast;
    // Installed builds show under the app's own name (its AUMID is set on the
    // Start-menu shortcut by the installer). `tauri dev` isn't installed, so
    // borrow PowerShell's id there or Windows silently drops the toast.
    let app_id = if cfg!(debug_assertions) {
        Toast::POWERSHELL_APP_ID.to_owned()
    } else {
        app.config().identifier.clone()
    };
    Toast::new(&app_id)
        .title(title)
        .text1(body)
        .on_activated(move |_| {
            on_click();
            Ok(())
        })
        .show()
        .map_err(|e| e.to_string())
}

#[cfg(not(windows))]
fn toast<F>(app: &AppHandle, title: &str, body: &str, _on_click: F) -> Result<(), String>
where
    F: Fn() + Send + 'static,
{
    // macOS / Linux: plain plugin notification (no click routing there yet).
    use tauri_plugin_notification::NotificationExt;
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|e| e.to_string())
}

#[cfg(windows)]
fn reveal_in_explorer(path: &Path) {
    let _ = std::process::Command::new("explorer")
        .arg(format!("/select,{}", path.display()))
        .spawn();
}

#[cfg(not(windows))]
fn reveal_in_explorer(_path: &Path) {}

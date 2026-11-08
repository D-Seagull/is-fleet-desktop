use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    webview::DownloadEvent,
    Manager, Url, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_opener::OpenerExt;

mod notify;

/// Bring the main window back to the foreground.
pub(crate) fn show_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Check GitHub Releases for a newer shell, download it in the background,
/// then ask before restarting. Lives in Rust on purpose: the page is loaded
/// from the web, so JS would also run in plain browsers.
#[cfg(desktop)]
async fn check_for_update(app: tauri::AppHandle) -> tauri_plugin_updater::Result<()> {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
    use tauri_plugin_updater::UpdaterExt;

    let Some(update) = app.updater()?.check().await? else {
        return Ok(());
    };
    let bytes = update.download(|_, _| {}, || {}).await?;

    let handle = app.clone();
    app.dialog()
        .message(format!(
            "IS Fleet {} is ready to install. Restart now?",
            update.version
        ))
        .title("Update available")
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Restart".into(),
            "Later".into(),
        ))
        .show(move |restart| {
            if !restart {
                return;
            }
            // On Windows this launches the installer and exits the process.
            match update.install(&bytes) {
                Ok(()) => handle.restart(),
                Err(e) => eprintln!("[updater] install failed: {e}"),
            }
        });
    Ok(())
}

/// Origins the main window may show: the web app it wraps (`frontendDist`)
/// and, in dev builds, the local dev server (`devUrl`). Read from
/// tauri.conf.json so the domain lives in one place.
fn app_origins(app: &tauri::App) -> Vec<String> {
    let build = &app.config().build;
    let mut urls: Vec<&Url> = Vec::new();
    if let Some(tauri::utils::config::FrontendDist::Url(u)) = &build.frontend_dist {
        urls.push(u);
    }
    if cfg!(debug_assertions) {
        if let Some(u) = &build.dev_url {
            urls.push(u);
        }
    }
    urls.iter().map(|u| u.origin().ascii_serialization()).collect()
}

/// Navigation guard for the main window. Our own pages load in place; file
/// downloads (the page navigates to a signed Supabase Storage URL, which the
/// download handler then saves) pass too. Anything else — a link in a chat
/// message, a redirect — opens in the system browser instead, so a foreign
/// page never runs inside the shell next to its native commands.
fn allow_navigation(app: &tauri::AppHandle, origins: &[String], url: &Url) -> bool {
    match url.scheme() {
        // Internal webview pages (blank/new tab, blob/data previews).
        "about" | "blob" | "data" => return true,
        "http" | "https" => {}
        // mailto:, tel:, etc. → the OS handler.
        _ => {
            let _ = app.opener().open_url(url.as_str(), None::<&str>);
            return false;
        }
    }
    if origins.contains(&url.origin().ascii_serialization()) {
        return true;
    }
    if url.scheme() == "https" && url.path().starts_with("/storage/v1/object/") {
        return true;
    }
    let _ = app.opener().open_url(url.as_str(), None::<&str>);
    false
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    // Must be the first plugin: a second launch (Start menu, or a click on an
    // older toast from the notification centre) just brings this instance
    // back instead of opening another copy.
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        show_main(app);
    }));
    builder
        // Lets the web page raise native OS notifications for new messages.
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        // Rust-side only (no JS permission): opens foreign links in the
        // system browser from the navigation guard.
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        // Windows toasts with a working click (see notify.rs).
        .invoke_handler(tauri::generate_handler![
            notify::show_notification,
            notify::reveal_download
        ])
        .setup(|app| {
            // ── Main window ──────────────────────────────────────────────
            // Built here (tauri.conf.json has it with "create": false) because
            // the download handler can only be attached while building it.
            // Downloads go to the user's Downloads folder, then a toast says
            // so; clicking it reveals the file.
            let window_config = app
                .config()
                .app
                .windows
                .iter()
                .find(|w| w.label == "main")
                .cloned()
                .expect("main window config");
            let downloads = app.handle().clone();
            let nav_app = app.handle().clone();
            let origins = app_origins(app);
            WebviewWindowBuilder::from_config(app.handle(), &window_config)?
                .on_navigation(move |url| allow_navigation(&nav_app, &origins, url))
                .on_download(move |_webview, event| {
                    match event {
                        DownloadEvent::Requested { url, destination } => {
                            notify::download_requested(&downloads, &url, destination)
                        }
                        DownloadEvent::Finished { url, path, success } => {
                            notify::download_finished(&downloads, &url, path, success)
                        }
                        _ => {}
                    }
                    true // allow every download
                })
                .build()?;

            #[cfg(desktop)]
            {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = check_for_update(handle).await {
                        eprintln!("[updater] {e}");
                    }
                });
            }

            // ── System tray ──────────────────────────────────────────────
            let show = MenuItem::with_id(app, "show", "Open IS Fleet", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;

            TrayIconBuilder::with_id("main-tray")
                .tooltip("IS Fleet")
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    // Left-click the tray icon → reopen the window.
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main(tray.app_handle());
                    }
                })
                .build(app)?;

            Ok(())
        })
        // Close button hides to tray instead of quitting — keeps the socket
        // alive in the background so messages keep arriving (Viber-style).
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running IS Fleet desktop");
}

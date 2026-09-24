use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};

/// Bring the main window back to the foreground.
fn show_main(app: &tauri::AppHandle) {
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Lets the web page raise native OS notifications for new messages.
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
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

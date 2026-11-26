//! Иконка в трее и «сворачивание в трей» (по образцу Anivox-Wraper).
//!
//! Левый клик — скрыть видимое окно / показать скрытое. Меню: «Открыть»,
//! «Сворачивать в трей» (галочка, хранится в tray.json, по умолчанию включено), «Выход».
//! Крестик окна при включённой настройке прячет окно в трей, «Выход» закрывает приложение.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Manager, WebviewWindow, WindowEvent};

const MAIN: &str = "main";

pub struct TrayState {
    pub minimize_to_tray: AtomicBool,
    path: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct TraySettings {
    minimize_to_tray: bool,
}

fn load(path: &PathBuf) -> bool {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<TraySettings>(&bytes).ok())
        .map(|s| s.minimize_to_tray)
        .unwrap_or(true)
}

fn save(path: &PathBuf, enabled: bool) -> Result<(), String> {
    let parent = path.parent().ok_or("нет каталога настроек")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(&TraySettings { minimize_to_tray: enabled }).map_err(|e| e.to_string())?;
    // Пишем во временный файл и переименовываем, чтобы не оставить битый tray.json
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn main_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(MAIN)
}

pub fn show(app: &AppHandle) {
    if let Some(w) = main_window(app) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn toggle(app: &AppHandle) {
    if let Some(w) = main_window(app) {
        let visible = w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false);
        if visible {
            let _ = w.hide();
        } else {
            show(app);
        }
    }
}

pub fn setup(app: &mut App) -> tauri::Result<()> {
    let path = app.path().app_config_dir()?.join("tray.json");
    let enabled = load(&path);
    app.manage(TrayState {
        minimize_to_tray: AtomicBool::new(enabled),
        path,
    });

    let open = MenuItem::with_id(app, "open", "Открыть", true, None::<&str>)?;
    let minimize = CheckMenuItem::with_id(app, "minimize", "Сворачивать в трей", true, enabled, None::<&str>)?;
    let updates = MenuItem::with_id(app, "updates", "Проверить обновления", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&open, &minimize, &updates, &PredefinedMenuItem::separator(app)?, &quit],
    )?;

    let minimize_item = minimize.clone();
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("app icon"))
        .tooltip("Lampa")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "open" => show(app),
            "minimize" => {
                let state = app.state::<TrayState>();
                let enabled = !state.minimize_to_tray.load(Ordering::Relaxed);
                match save(&state.path, enabled) {
                    Ok(()) => state.minimize_to_tray.store(enabled, Ordering::Relaxed),
                    Err(err) => eprintln!("Не удалось сохранить настройку трея: {err}"),
                }
                // CheckMenuItem сам переключает галочку — приводим её к реальному состоянию
                let _ = minimize_item.set_checked(state.minimize_to_tray.load(Ordering::Relaxed));
            }
            "updates" => crate::updater::check_in_background(app, true),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

/// Крестик окна: спрятать в трей вместо выхода, если настройка включена.
pub fn on_window_event(window: &tauri::Window, event: &WindowEvent) {
    if window.label() != MAIN {
        return;
    }
    if let WindowEvent::CloseRequested { api, .. } = event {
        let state = window.state::<TrayState>();
        if state.minimize_to_tray.load(Ordering::Relaxed) {
            api.prevent_close();
            let _ = window.hide();
        }
    }
}

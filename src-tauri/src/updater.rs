//! Обновление приложения через GitHub Releases (tauri-plugin-updater).
//!
//! `latest.json` последнего релиза сравнивается с текущей версией. Если есть новее —
//! спрашиваем пользователя, скачиваем установщик (подпись проверяется ключом из
//! tauri.conf.json), ставим в passive-режиме и перезапускаемся.
//! При запуске проверка тихая (ошибки и «обновлений нет» не показываются),
//! из трея — с ответом в любом случае.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::AppHandle;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_updater::UpdaterExt;

static CHECKING: AtomicBool = AtomicBool::new(false);

pub fn check_in_background(app: &AppHandle, manual: bool) {
    if CHECKING.swap(true, Ordering::SeqCst) {
        if manual {
            info(app, "Проверка обновлений уже идёт.");
        }
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(err) = check(&app, manual).await {
            eprintln!("Обновление: {err}");
            if manual {
                error(&app, &format!("Не удалось проверить обновления:\n{err}"));
            }
        }
        CHECKING.store(false, Ordering::SeqCst);
    });
}

async fn check(app: &AppHandle, manual: bool) -> Result<(), String> {
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;

    let Some(update) = update else {
        if manual {
            info(app, &format!("У вас последняя версия ({}).", app.package_info().version));
        }
        return Ok(());
    };

    let mut text = format!(
        "Доступна новая версия Lampa: {} (у вас {}).",
        update.version,
        app.package_info().version
    );
    if let Some(notes) = update.body.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        let notes: String = notes.chars().take(1500).collect();
        text.push_str("\n\n");
        text.push_str(&notes);
    }
    text.push_str("\n\nУстановить сейчас? Приложение перезапустится.");

    let (tx, rx) = tokio_oneshot();
    app.dialog()
        .message(text)
        .title("Обновление Lampa")
        .kind(MessageDialogKind::Info)
        .buttons(MessageDialogButtons::OkCancelCustom("Обновить".into(), "Позже".into()))
        .show(move |ok| {
            let _ = tx.send(ok);
        });
    if !rx.await.unwrap_or(false) {
        return Ok(());
    }

    update
        .download_and_install(|_chunk, _total| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    // На Windows установщик сам завершает приложение; на остальных — перезапуск
    app.restart();
}

fn tokio_oneshot() -> (
    std::sync::mpsc::Sender<bool>,
    impl std::future::Future<Output = Result<bool, ()>>,
) {
    let (tx, rx) = std::sync::mpsc::channel();
    let fut = async move {
        tauri::async_runtime::spawn_blocking(move || rx.recv().map_err(|_| ()))
            .await
            .map_err(|_| ())?
    };
    (tx, fut)
}

fn info(app: &AppHandle, text: &str) {
    app.dialog()
        .message(text)
        .title("Обновление Lampa")
        .kind(MessageDialogKind::Info)
        .show(|_| {});
}

fn error(app: &AppHandle, text: &str) {
    app.dialog()
        .message(text)
        .title("Обновление Lampa")
        .kind(MessageDialogKind::Error)
        .show(|_| {});
}

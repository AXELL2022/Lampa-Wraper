mod external;
mod tray;
mod updater;

const INIT_SCRIPT: &str = include_str!("init.js");

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Повторный запуск (например, ярлык при окне в трее) показывает уже открытое окно
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| tray::show(app)))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(
            tauri::plugin::Builder::<tauri::Wry, ()>::new("lampa-shim")
                .js_init_script(INIT_SCRIPT.to_string())
                .build(),
        )
        .manage(external::Processes::default())
        .setup(|app| {
            tray::setup(app)?;
            // Debug-сборки (tauri dev) не трогают релизы
            if !cfg!(debug_assertions) {
                updater::check_in_background(app.handle(), false);
            }
            Ok(())
        })
        .on_window_event(tray::on_window_event)
        .invoke_handler(tauri::generate_handler![
            external::lampa_spawn,
            external::lampa_kill,
            external::lampa_local_get,
            external::lampa_ipc_exchange,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Lampa");
}

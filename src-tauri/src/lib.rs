mod external;

const INIT_SCRIPT: &str = include_str!("init.js");

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri::plugin::Builder::<tauri::Wry, ()>::new("lampa-shim")
                .js_init_script(INIT_SCRIPT.to_string())
                .build(),
        )
        .manage(external::Processes::default())
        .invoke_handler(tauri::generate_handler![
            external::lampa_spawn,
            external::lampa_kill,
            external::lampa_local_get,
            external::lampa_ipc_exchange,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Lampa");
}

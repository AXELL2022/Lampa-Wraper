// Скрываем консольное окно в release-сборке на Windows
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    lampa_lib::run()
}

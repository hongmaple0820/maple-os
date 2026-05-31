#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::menu::{Menu, MenuItem, Submenu};
use tauri_plugin_shell::ShellExt;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            // Build menu
            let file_menu = Submenu::with_items(
                app,
                "文件",
                true,
                &[
                    &MenuItem::with_id(app, "new-chat", "新建对话", true, None::<&str>)?,
                    &MenuItem::with_id(app, "open-workspace", "打开工作空间", true, None::<&str>)?,
                    &MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?,
                ],
            )?;
            let help_menu = Submenu::with_items(
                app,
                "帮助",
                true,
                &[
                    &MenuItem::with_id(app, "about", "关于 MapleOS", true, None::<&str>)?,
                ],
            )?;
            let menu = Menu::with_items(app, &[&file_menu, &help_menu])?;
            app.set_menu(menu)?;

            // Start sidecar
            let sidecar = app.shell().sidecar("mapleos-server").unwrap();
            let (mut rx, _) = sidecar.spawn().expect("failed to start mapleos-server sidecar");
            tauri::async_runtime::spawn(async move {
                while let Some(event) = rx.recv().await {
                    if let tauri_plugin_shell::Event::Stderr(line) = event {
                        eprintln!("[server] {}", line);
                    }
                }
            });
            Ok(())
        })
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![greet, get_system_info])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! Welcome to MapleOS.", name)
}

#[tauri::command]
fn get_system_info() -> serde_json::Value {
    serde_json::json!({
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "version": "0.1.0",
    })
}

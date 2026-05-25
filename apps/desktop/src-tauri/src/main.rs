#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{Manager, Menu, MenuItem, Submenu};

fn main() {
    let menu = Menu::new()
        .item(&MenuItem::new("MapleOS", true, None))
        .item(&Submenu::new("文件", Menu::new()
            .item(&MenuItem::new("新建对话", false, Some("new-chat")))
            .item(&MenuItem::new("打开工作空间", false, Some("open-workspace")))
            .separator()
            .item(&MenuItem::new("设置", false, Some("settings")))
        ))
        .item(&Submenu::new("帮助", Menu::new()
            .item(&MenuItem::new("关于 MapleOS", false, Some("about")))
        ));

    tauri::Builder::default()
        .menu(menu)
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![greet, get_system_info])
        .setup(|app| {
            let sidecar = app.shell().sidecar("mapleos-server").unwrap();
            let (mut rx, _) = sidecar.spawn().expect("failed to start mapleos-server sidecar");
            tauri::async_runtime::spawn(async move {
                while let Some(event) = rx.recv().await {
                    if let tauri_plugin_shell::ShellEvent::Stderr(line) = event {
                        eprintln!("[server] {}", line);
                    }
                }
            });
            Ok(())
        })
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
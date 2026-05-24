#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::Command;
use tauri::{Manager, Menu, MenuItem, Submenu};

fn start_backend() -> std::process::Child {
    Command::new("mapleos-server")
        .env("PORT", "7788")
        .spawn()
        .expect("failed to start mapleos-server")
}

fn start_bridge() -> std::process::Child {
    Command::new("node")
        .arg("bridge/bridge-http.mjs")
        .spawn()
        .expect("failed to start bridge-http")
}

fn main() {
    let mut backend = start_backend();
    let mut bridge = start_bridge();

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
        .on_window_event(|event| {
            if let tauri::WindowEvent::Destroyed = event.event() {
                let _ = backend.kill();
                let _ = bridge.kill();
            }
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
use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri_plugin_shell::ShellExt;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![greet, get_system_info])
        .setup(|app| {
            let file_submenu = SubmenuBuilder::new(app, "文件")
                .text("new-chat", "新建对话")
                .text("open-workspace", "打开工作空间")
                .separator()
                .text("settings", "设置")
                .build()?;
            let help_submenu = SubmenuBuilder::new(app, "帮助")
                .text("about", "关于 MapleOS")
                .build()?;
            let menu = MenuBuilder::new(app)
                .items(&[
                    &PredefinedMenuItem::about(app, None, None)?,
                    &file_submenu,
                    &help_submenu,
                ])
                .build()?;
            app.set_menu(menu)?;

            app.on_menu_event(|app, event| {
                match event.id().as_ref() {
                    "new-chat" => {
                        // TODO: handle new chat
                    }
                    "open-workspace" => {
                        // TODO: handle open workspace
                    }
                    "settings" => {
                        // TODO: handle settings
                    }
                    "about" => {
                        // TODO: handle about
                    }
                    _ => {}
                }
            });

            // Start mapleos-server as sidecar
            let sidecar = app
                .shell()
                .sidecar("mapleos-server")
                .expect("failed to find mapleos-server sidecar");
            let (mut rx, _child) = sidecar.spawn().expect("failed to start mapleos-server sidecar");
            tauri::async_runtime::spawn(async move {
                use tauri_plugin_shell::process::CommandEvent;
                while let Some(event) = rx.recv().await {
                    if let CommandEvent::Stderr(line) = event {
                        eprintln!("[server] {}", String::from_utf8_lossy(&line));
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
        "version": "0.1.0"
    })
}

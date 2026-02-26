use log::LevelFilter;
use tauri::async_runtime::spawn;
use tauri::Emitter;
use tokio::time::{sleep, Duration};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(LevelFilter::Debug)
                .level_for("tauri-plugin-app-events", log::LevelFilter::Trace)
                .build(),
        )
        .setup(|app| {
            #[cfg(mobile)]
            {
                use tauri::ipc::Channel;
                use tauri_plugin_app_events::*;

                log::info!("Running on mobile mode.");

                app.handle().plugin(tauri_plugin_biometry::init())?;
                app.handle().plugin(tauri_plugin_barcode_scanner::init())?;

                let app_handle = app.handle();
                app_handle.plugin(tauri_plugin_app_events::init())?;
                let app_handle = app.handle().clone();

                app_handle.app_events().set_resume_handler(Channel::new({
                    let app_handle = app_handle.clone();
                    move |_| {
                        log::info!("onResumed from Rust");

                        // run async work without blocking the handler
                        let app_handle = app_handle.clone();
                        spawn(async move {
                            let nonce: u64 = rand::random();
                            for _ in 0..10 {
                                // ignore errors if no listeners; remove `let _ =` if you want to propagate
                                let _ = app_handle.emit("resumed", nonce);
                                tokio::time::sleep(Duration::from_millis(50)).await;
                            }
                        });

                        Ok(())
                    }
                }))?;
                let app_handle = app.handle().clone();

                app_handle.app_events().set_pause_handler(Channel::new({
                    let app_handle = app_handle.clone();
                    move |_| {
                        log::info!("onPaused from Rust");

                        // run async work without blocking the handler
                        let app_handle = app_handle.clone();
                        spawn(async move {
                            let nonce: u64 = rand::random();
                            for _ in 0..10 {
                                // ignore errors if no listeners; remove `let _ =` if you want to propagate
                                let _ = app_handle.emit("paused", nonce);
                                tokio::time::sleep(Duration::from_millis(50)).await;
                            }
                        });

                        Ok(())
                    }
                }))?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

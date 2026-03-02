#![warn(clippy::all, rust_2018_idioms)]
// hide console window on Windows in release mode
#![cfg_attr(
    all(target_os = "windows", not(feature = "console")),
    windows_subsystem = "windows"
)]

use cfg_if::cfg_if;
use kaspa_ng_core::app::{ApplicationContext, kaspa_ng_main};
use workflow_log::*;

#[cfg(all(not(target_arch = "wasm32"), target_os = "linux"))]
fn sanitize_linux_ld_library_path() {
    use std::ffi::OsString;
    use std::path::PathBuf;

    let Some(current) = std::env::var_os("LD_LIBRARY_PATH") else {
        return;
    };

    let entries: Vec<PathBuf> = std::env::split_paths(&current).collect();
    if entries.is_empty() {
        return;
    }

    let mut filtered: Vec<PathBuf> = Vec::with_capacity(entries.len());
    let mut removed: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let text = entry.to_string_lossy();
        let is_snap_core = text.contains("/snap/core");
        if is_snap_core {
            removed.push(entry);
        } else {
            filtered.push(entry);
        }
    }

    if removed.is_empty() {
        return;
    }

    let removed_msg = removed
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(", ");

    if filtered.is_empty() {
        unsafe {
            std::env::remove_var("LD_LIBRARY_PATH");
        }
        log_warn!(
            "Removed incompatible LD_LIBRARY_PATH entries for WebKitGTK: {removed_msg} (unset LD_LIBRARY_PATH)"
        );
    } else {
        let joined: OsString = std::env::join_paths(filtered).unwrap_or_default();
        unsafe {
            std::env::set_var("LD_LIBRARY_PATH", joined);
        }
        log_warn!("Removed incompatible LD_LIBRARY_PATH entries for WebKitGTK: {removed_msg}");
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "linux"))]
fn sanitize_linux_snap_environment() {
    fn set_env_var<K: AsRef<std::ffi::OsStr>, V: AsRef<std::ffi::OsStr>>(key: K, value: V) {
        unsafe {
            std::env::set_var(key, value);
        }
    }
    fn remove_env_var<K: AsRef<std::ffi::OsStr>>(key: K) {
        unsafe {
            std::env::remove_var(key);
        }
    }

    // When kaspa-ng is launched from a Snap-hosted shell (e.g. VSCode snap),
    // Snap-specific environment variables can leak incompatible glibc/runtime
    // paths into WebKitGTK helper processes and crash embedded WebViews.
    let mut removed_snap_keys: Vec<String> = Vec::new();
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("SNAP") {
            removed_snap_keys.push(key.to_string_lossy().into_owned());
            remove_env_var(key);
        }
    }

    for key in [
        "GTK_PATH",
        "GTK_EXE_PREFIX",
        "GTK_IM_MODULE_FILE",
        "GIO_MODULE_DIR",
    ] {
        if std::env::var_os(key).is_some() {
            remove_env_var(key);
        }
    }

    if let Some(orig) = std::env::var_os("XDG_DATA_DIRS_VSCODE_SNAP_ORIG") {
        set_env_var("XDG_DATA_DIRS", orig);
    }
    if let Some(orig) = std::env::var_os("XDG_CONFIG_DIRS_VSCODE_SNAP_ORIG") {
        set_env_var("XDG_CONFIG_DIRS", orig);
    }

    if !removed_snap_keys.is_empty() {
        log_warn!(
            "Sanitized Snap shell environment for WebKitGTK compatibility ({})",
            removed_snap_keys.join(", ")
        );
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "linux"))]
fn configure_linux_webkit_runtime() {
    // Workaround for known WebKitGTK instability on Linux/NVIDIA with DMABuf renderer.
    // Can be overridden by explicitly setting WEBKIT_DISABLE_DMABUF_RENDERER.
    let is_nvidia = std::path::Path::new("/proc/driver/nvidia/version").exists();
    if is_nvidia && std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        unsafe {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
        log_warn!("Enabled WEBKIT_DISABLE_DMABUF_RENDERER=1 (Linux/NVIDIA WebKitGTK stability workaround)");
    }
}

cfg_if! {
    if #[cfg(not(target_arch = "wasm32"))] {

        fn main() {

            #[cfg(feature = "console")] {
                unsafe {
                    std::env::set_var("RUST_BACKTRACE", "full");
                }
            }

            #[cfg(target_os = "linux")]
            sanitize_linux_snap_environment();

            #[cfg(target_os = "linux")]
            sanitize_linux_ld_library_path();

            #[cfg(target_os = "linux")]
            configure_linux_webkit_runtime();

            kaspa_alloc::init_allocator_with_default_settings();

            let body = async {
                if let Err(err) = kaspa_ng_main(ApplicationContext::default()).await {
                    log_error!("Error: {err}");
                }
            };

            #[allow(clippy::expect_used, clippy::diverging_sub_expression)]
            //{
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("Failed building the Runtime")
                    .block_on(body);
            //};

            #[cfg(feature = "console")]
            {
                println!("Press Enter to exit...");
                let mut input = String::new();
                std::io::stdin().read_line(&mut input).expect("Failed to read line");
            }


        }

    } else {

        fn main() {

            wasm_bindgen_futures::spawn_local(async {
                if let Err(err) = kaspa_ng_main(ApplicationContext::default()).await {
                    log_error!("Error: {err}");
                }
            });

        }
    }
}

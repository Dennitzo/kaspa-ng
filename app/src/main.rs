#![warn(clippy::all, rust_2018_idioms)]
// hide console window on Windows in release mode
#![cfg_attr(
    all(target_os = "windows", not(feature = "console")),
    windows_subsystem = "windows"
)]

use cfg_if::cfg_if;
use kaspa_ng_core::app::{ApplicationContext, kaspa_ng_main};
#[cfg(all(not(target_arch = "wasm32"), target_os = "linux"))]
use kaspa_ng_core::settings::{RenderingSettings, Settings};
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
fn load_rendering_settings_for_startup() -> RenderingSettings {
    match Settings::load_for_network_sync(kaspa_ng_core::network::Network::Mainnet) {
        Ok(settings) => settings.rendering,
        Err(err) => {
            log_warn!("Unable to load rendering settings for startup: {err}");
            RenderingSettings::default()
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "linux"))]
fn linux_hardware_rendering_available(
    is_nvidia_proprietary: bool,
    is_nvidia_nouveau: bool,
) -> bool {
    if is_nvidia_proprietary || is_nvidia_nouveau {
        return true;
    }

    std::fs::read_dir("/dev/dri")
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .any(|name| name.starts_with("renderD") || name.starts_with("card"))
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "linux"))]
fn configure_linux_webkit_runtime(rendering: &RenderingSettings) {
    // Workaround for known WebKitGTK instability on Linux/NVIDIA with DMABuf renderer.
    // Applies to both proprietary NVIDIA and nouveau driver stacks.
    // Can be overridden by explicitly setting WEBKIT_DISABLE_DMABUF_RENDERER.
    let is_nvidia_proprietary = std::path::Path::new("/proc/driver/nvidia/version").exists();
    let is_nvidia_nouveau = std::path::Path::new("/sys/module/nouveau").exists();
    if (is_nvidia_proprietary || is_nvidia_nouveau)
        && std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none()
    {
        unsafe {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
        let driver = if is_nvidia_proprietary {
            "nvidia"
        } else {
            "nouveau"
        };
        log_warn!(
            "Enabled WEBKIT_DISABLE_DMABUF_RENDERER=1 (Linux/{driver} WebKitGTK stability workaround)"
        );
    }

    let hardware_available =
        linux_hardware_rendering_available(is_nvidia_proprietary, is_nvidia_nouveau);
    let should_force_software = !rendering.hardware_acceleration || !hardware_available;

    if should_force_software && std::env::var_os("LIBGL_ALWAYS_SOFTWARE").is_none() {
        unsafe {
            std::env::set_var("LIBGL_ALWAYS_SOFTWARE", "1");
        }
        if !rendering.hardware_acceleration {
            log_warn!(
                "Enabled LIBGL_ALWAYS_SOFTWARE=1 (hardware acceleration disabled in settings)"
            );
        } else {
            log_warn!(
                "Enabled LIBGL_ALWAYS_SOFTWARE=1 (no hardware rendering device detected, fallback active)"
            );
        }
    } else if rendering.hardware_acceleration {
        if hardware_available {
            log_warn!("Hardware rendering is enabled and a GPU device is available");
        }
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
            {
                let rendering = load_rendering_settings_for_startup();
                configure_linux_webkit_runtime(&rendering);
            }

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

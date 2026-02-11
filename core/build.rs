use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;
use vergen::EmitBuilder;

// https://docs.rs/vergen/latest/vergen/struct.EmitBuilder.html#method.emit
fn main() -> Result<(), Box<dyn Error>> {
    EmitBuilder::builder()
        .all_build()
        .all_cargo()
        .all_git()
        .all_rustc()
        .emit()?;

    export_rusty_kaspa_workspace_version()?;
    build_explorer_if_needed()?;
    build_stratum_bridge_if_needed()?;
    Ok(())
}

fn export_rusty_kaspa_workspace_version() -> Result<(), Box<dyn Error>> {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let repo_root = manifest_dir
        .parent()
        .ok_or("failed to resolve repo root")?
        .to_path_buf();
    let rusty_toml = repo_root.join("rusty-kaspa").join("Cargo.toml");

    println!("cargo:rerun-if-changed={}", rusty_toml.display());

    let contents = std::fs::read_to_string(&rusty_toml)?;
    if let Some(version) = parse_workspace_version(&contents) {
        println!("cargo:rustc-env=RUSTY_KASPA_WORKSPACE_VERSION={version}");
    } else {
        println!("cargo:warning=Unable to parse Rusty Kaspa workspace version");
    }

    Ok(())
}

fn parse_workspace_version(contents: &str) -> Option<String> {
    let mut in_workspace_package = false;
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_workspace_package = line == "[workspace.package]";
            continue;
        }
        if in_workspace_package && line.starts_with("version") {
            let (_, rhs) = line.split_once('=')?;
            let rhs = rhs.trim();
            if let Some(stripped) = rhs.strip_prefix('"')
                && let Some(end) = stripped.find('"')
            {
                return Some(stripped[..end].to_string());
            }
            if let Some(stripped) = rhs.strip_prefix('\'')
                && let Some(end) = stripped.find('\'')
            {
                return Some(stripped[..end].to_string());
            }
        }
    }
    None
}

fn build_explorer_if_needed() -> Result<(), Box<dyn Error>> {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let repo_root = manifest_dir
        .parent()
        .ok_or("failed to resolve repo root")?
        .to_path_buf();
    let explorer_root = repo_root.join("kaspa-explorer-ng");
    if !explorer_root.exists() {
        return Ok(());
    }

    let package_json = explorer_root.join("package.json");
    let router_config = explorer_root.join("react-router.config.ts");
    let app_dir = explorer_root.join("app");
    let public_dir = explorer_root.join("public");
    let build_index = explorer_root
        .join("build")
        .join("client")
        .join("index.html");

    println!("cargo:rerun-if-changed={}", package_json.display());
    println!("cargo:rerun-if-changed={}", router_config.display());
    println!("cargo:rerun-if-changed={}", app_dir.display());
    println!("cargo:rerun-if-changed={}", public_dir.display());

    let latest_src = newest_mtime(&package_json)
        .into_iter()
        .chain(newest_mtime(&router_config))
        .chain(newest_mtime(&app_dir))
        .chain(newest_mtime(&public_dir))
        .max();

    if build_index.exists()
        && let (Some(bin_time), Some(src_time)) = (mtime(&build_index), latest_src)
        && bin_time >= src_time
    {
        sync_explorer_build(&explorer_root, &repo_root)?;
        return Ok(());
    }

    println!("cargo:warning=Building kaspa-explorer-ng (static)...");
    let npm = std::env::var("NPM").unwrap_or_else(|_| "npm".to_string());
    let node_modules = explorer_root.join("node_modules");

    if !node_modules.exists() {
        let status = Command::new(&npm)
            .current_dir(&explorer_root)
            .args(["install"])
            .status();
        if status.map(|s| !s.success()).unwrap_or(true) {
            println!("cargo:warning=kaspa-explorer-ng npm install failed; skipping build");
            return Ok(());
        }
    }

    let status = Command::new(&npm)
        .current_dir(&explorer_root)
        .args(["run", "build"])
        .status();

    if status.map(|s| !s.success()).unwrap_or(true) {
        println!("cargo:warning=kaspa-explorer-ng build failed; skipping");
    }

    sync_explorer_build(&explorer_root, &repo_root)?;

    Ok(())
}

fn build_stratum_bridge_if_needed() -> Result<(), Box<dyn Error>> {
    let host = std::env::var("HOST").unwrap_or_default();
    let target = std::env::var("TARGET").unwrap_or_default();
    if !host.is_empty() && !target.is_empty() && host != target {
        println!("cargo:warning=Skipping stratum-bridge build (cross-compile: {host} -> {target})");
        return Ok(());
    }

    if target.contains("wasm32") {
        println!("cargo:warning=Skipping stratum-bridge build (wasm32 target)");
        return Ok(());
    }

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let repo_root = manifest_dir
        .parent()
        .ok_or("failed to resolve repo root")?
        .to_path_buf();
    let rusty_kaspa = repo_root.join("rusty-kaspa");

    let bridge_src = rusty_kaspa.join("bridge").join("src");
    let bridge_toml = rusty_kaspa.join("bridge").join("Cargo.toml");
    let rusty_toml = rusty_kaspa.join("Cargo.toml");

    println!("cargo:rerun-if-changed={}", bridge_toml.display());
    println!("cargo:rerun-if-changed={}", rusty_toml.display());
    println!("cargo:rerun-if-changed={}", bridge_src.display());

    let bin_name = if cfg!(windows) {
        "stratum-bridge.exe"
    } else {
        "stratum-bridge"
    };
    let bin_path = rusty_kaspa.join("target").join("release").join(bin_name);

    let latest_src = newest_mtime(&bridge_src)
        .into_iter()
        .chain(newest_mtime(&bridge_toml))
        .chain(newest_mtime(&rusty_toml))
        .max();

    if bin_path.exists()
        && let (Some(bin_time), Some(src_time)) = (mtime(&bin_path), latest_src)
        && bin_time >= src_time
    {
        sync_stratum_bridge_binary(&bin_path, &repo_root)?;
        return Ok(());
    }

    println!("cargo:warning=Building kaspa-stratum-bridge (release)...");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = Command::new(cargo)
        .current_dir(&rusty_kaspa)
        .args(["build", "-p", "kaspa-stratum-bridge", "--release"])
        .status()?;

    if !status.success() {
        return Err("failed to build kaspa-stratum-bridge".into());
    }

    sync_stratum_bridge_binary(&bin_path, &repo_root)?;

    Ok(())
}

fn sync_explorer_build(explorer_root: &Path, repo_root: &Path) -> Result<(), Box<dyn Error>> {
    let build_root = explorer_root.join("build");
    let dist_root = explorer_root.join("dist");

    let (src_root, dest_root) = if build_root.join("client").join("index.html").exists() {
        let dest = target_profile_dir(repo_root)
            .join("kaspa-explorer-ng")
            .join("build");
        (build_root, dest)
    } else if dist_root.join("index.html").exists() {
        let dest = target_profile_dir(repo_root)
            .join("kaspa-explorer-ng")
            .join("dist");
        (dist_root, dest)
    } else {
        return Ok(());
    };

    let src_time = newest_mtime(&src_root);
    let dest_time = newest_mtime(&dest_root);
    if dest_root.exists() && dest_time.is_some() && src_time.is_some() && dest_time >= src_time {
        return Ok(());
    }

    if dest_root.exists() {
        std::fs::remove_dir_all(&dest_root)?;
    }
    copy_dir_all(&src_root, &dest_root)?;
    Ok(())
}

fn sync_stratum_bridge_binary(bin_path: &Path, repo_root: &Path) -> Result<(), Box<dyn Error>> {
    if !bin_path.exists() {
        return Ok(());
    }

    let target_dir = target_profile_dir(repo_root);
    std::fs::create_dir_all(&target_dir)?;
    let dest_path = target_dir.join(
        bin_path
            .file_name()
            .ok_or("failed to resolve stratum-bridge filename")?,
    );

    let src_time = mtime(bin_path);
    let dest_time = mtime(&dest_path);
    if dest_path.exists() && dest_time.is_some() && src_time.is_some() && dest_time >= src_time {
        return Ok(());
    }

    std::fs::copy(bin_path, dest_path)?;
    Ok(())
}

fn target_profile_dir(repo_root: &Path) -> PathBuf {
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "release".to_string());
    if let Ok(target_dir) = std::env::var("CARGO_TARGET_DIR") {
        return PathBuf::from(target_dir).join(profile);
    }
    let mut target_dir = repo_root.join("target");
    if let Ok(build_target) = std::env::var("CARGO_BUILD_TARGET") {
        if !build_target.is_empty() {
            target_dir = target_dir.join(build_target);
        }
    }
    target_dir.join(profile)
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_all(&path, &target)?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&path, &target)?;
        }
    }
    Ok(())
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn newest_mtime(path: &Path) -> Option<SystemTime> {
    if path.is_file() {
        return mtime(path);
    }
    let mut newest: Option<SystemTime> = None;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            let t = if p.is_dir() {
                newest_mtime(&p)
            } else {
                mtime(&p)
            };
            if let Some(t) = t {
                newest = Some(match newest {
                    Some(cur) if cur >= t => cur,
                    _ => t,
                });
            }
        }
    }
    newest
}

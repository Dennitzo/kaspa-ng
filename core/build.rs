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
    sync_external_repo_if_needed("K", "https://github.com/thesheepcat/K.git")?;
    sync_external_repo_if_needed("K-indexer", "https://github.com/thesheepcat/K-indexer.git")?;
    build_explorer_if_needed()?;
    if external_builds_enabled() {
        build_k_social_if_needed()?;
    }
    build_cpu_miner_if_needed()?;
    build_rothschild_if_needed()?;
    build_simply_kaspa_indexer_if_needed()?;
    if external_builds_enabled() {
        build_k_indexer_if_needed()?;
    }
    build_stratum_bridge_if_needed()?;
    Ok(())
}

fn external_builds_enabled() -> bool {
    !std::env::var("KASPA_NG_SKIP_EXTERNAL_BUILDS")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn sync_external_repo_if_needed(name: &str, url: &str) -> Result<(), Box<dyn Error>> {
    if std::env::var("KASPA_NG_SKIP_EXTERNAL_SYNC")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        return Ok(());
    }

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let repo_root = manifest_dir
        .parent()
        .ok_or("failed to resolve repo root")?
        .to_path_buf();
    let target = repo_root.join(name);

    if !target.exists() {
        println!("cargo:warning=Cloning {name}...");
        let status = Command::new("git")
            .current_dir(&repo_root)
            .args(["clone", "--depth", "1", url, name])
            .status();
        if status.map(|s| !s.success()).unwrap_or(true) {
            println!(
                "cargo:warning=Failed to clone {name}; continuing with existing workspace state"
            );
        }
        return Ok(());
    }

    if !target.join(".git").exists() {
        println!("cargo:warning=Skipping sync for {name} (not a git repository)");
        return Ok(());
    }

    let mut stashed = false;
    if has_local_git_changes(&target)? {
        println!("cargo:warning={name} has local changes; stashing before pull");
        let status = Command::new("git")
            .current_dir(&target)
            .args([
                "stash",
                "push",
                "--include-untracked",
                "-m",
                "kaspa-ng build auto-stash",
            ])
            .status();
        if status.map(|s| s.success()).unwrap_or(false) {
            stashed = true;
        } else {
            println!("cargo:warning=Failed to stash local changes for {name}; skipping pull");
            return Ok(());
        }
    }

    let status = Command::new("git")
        .current_dir(&target)
        .args(["pull", "--ff-only"])
        .status();
    if status.map(|s| !s.success()).unwrap_or(true) {
        println!("cargo:warning=Failed to update {name} via git pull --ff-only");
    }

    if stashed {
        let status = Command::new("git")
            .current_dir(&target)
            .args(["stash", "pop"])
            .status();
        if status.map(|s| !s.success()).unwrap_or(true) {
            println!("cargo:warning=Failed to re-apply stashed local changes for {name}");
        }
    }

    Ok(())
}

fn has_local_git_changes(repo_dir: &Path) -> Result<bool, Box<dyn Error>> {
    let output = Command::new("git")
        .current_dir(repo_dir)
        .args(["status", "--porcelain"])
        .output()?;
    Ok(!output.stdout.is_empty())
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
    let lockfile = explorer_root.join("package-lock.json");

    let npm_cmd = |args: &[&str]| {
        let mut cmd = Command::new(&npm);
        cmd.current_dir(&explorer_root)
            .env("npm_config_audit", "false")
            .env("npm_config_fund", "false")
            .args(args);
        cmd
    };

    if !node_modules.exists() {
        let status = if lockfile.exists() {
            npm_cmd(&["ci", "--no-audit", "--no-fund"]).status()
        } else {
            npm_cmd(&["install", "--no-audit", "--no-fund"]).status()
        };
        if status.map(|s| !s.success()).unwrap_or(true) {
            println!("cargo:warning=kaspa-explorer-ng npm install failed; skipping build");
            return Ok(());
        }
    }

    let status = npm_cmd(&["run", "build"]).status();

    if status.map(|s| !s.success()).unwrap_or(true) {
        println!("cargo:warning=kaspa-explorer-ng build failed; skipping");
    }

    sync_explorer_build(&explorer_root, &repo_root)?;

    Ok(())
}

fn build_k_social_if_needed() -> Result<(), Box<dyn Error>> {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let repo_root = manifest_dir
        .parent()
        .ok_or("failed to resolve repo root")?
        .to_path_buf();
    let k_root = repo_root.join("K");
    if !k_root.exists() {
        return Ok(());
    }

    let package_json = k_root.join("package.json");
    let lockfile = k_root.join("package-lock.json");
    let src_dir = k_root.join("src");
    let public_dir = k_root.join("public");
    let build_index = k_root.join("dist").join("index.html");

    println!("cargo:rerun-if-changed={}", package_json.display());
    println!("cargo:rerun-if-changed={}", lockfile.display());
    println!("cargo:rerun-if-changed={}", src_dir.display());
    println!("cargo:rerun-if-changed={}", public_dir.display());
    println!(
        "cargo:rerun-if-changed={}",
        k_root
            .join("src")
            .join("services")
            .join("kaspaService.ts")
            .display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        k_root
            .join("src")
            .join("contexts")
            .join("AuthContext.tsx")
            .display()
    );

    apply_k_runtime_patches(&k_root)?;

    let latest_src = newest_mtime(&package_json)
        .into_iter()
        .chain(newest_mtime(&lockfile))
        .chain(newest_mtime(&src_dir))
        .chain(newest_mtime(&public_dir))
        .max();

    if build_index.exists()
        && let (Some(bin_time), Some(src_time)) = (mtime(&build_index), latest_src)
        && bin_time >= src_time
    {
        sync_k_social_build(&k_root, &repo_root)?;
        return Ok(());
    }

    println!("cargo:warning=Building K (static)...");
    let npm = std::env::var("NPM").unwrap_or_else(|_| "npm".to_string());
    let node_modules = k_root.join("node_modules");

    if !node_modules.exists() {
        let status = if lockfile.exists() {
            Command::new(&npm)
                .current_dir(&k_root)
                .args(["ci", "--no-audit", "--no-fund"])
                .status()
        } else {
            Command::new(&npm)
                .current_dir(&k_root)
                .args(["install", "--no-audit", "--no-fund"])
                .status()
        };
        if status.map(|s| !s.success()).unwrap_or(true) {
            println!("cargo:warning=K npm install failed; skipping build");
            return Ok(());
        }
    }

    let status = Command::new(&npm)
        .current_dir(&k_root)
        .args(["run", "build"])
        .status();
    if status.map(|s| !s.success()).unwrap_or(true) {
        println!("cargo:warning=K build failed; skipping");
    }

    sync_k_social_build(&k_root, &repo_root)?;
    Ok(())
}

fn apply_k_runtime_patches(k_root: &Path) -> Result<(), Box<dyn Error>> {
    let enabled = std::env::var("KASPA_NG_ENABLE_K_PRIVATE_KEY_PATCH")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if !enabled {
        return Ok(());
    }

    println!("cargo:warning=Applying K private-key compatibility patch");

    let kaspa_service = k_root.join("src").join("services").join("kaspaService.ts");
    if kaspa_service.exists() {
        patch_file_replace_all(
            &kaspa_service,
            &[
                (
                    "privKey = privateKeyHex;",
                    "privKey = this.normalizePrivateKeyInput(privateKeyHex);",
                ),
                (
                    "new kaspa.PrivateKey(privateKeyHex);",
                    "new kaspa.PrivateKey(this.normalizePrivateKeyInput(privateKeyHex));",
                ),
                (
                    "      // Create private key object\n      const privateKeyObj = new kaspa.PrivateKey(privKey);\n      \n      // Get public key",
                    "      // Create private key object\n      const privateKeyObj = new kaspa.PrivateKey(privKey);\n      const normalizedPrivateKey = privateKeyObj.toString();\n      \n      // Get public key",
                ),
                (
                    "        privateKey: privKey,",
                    "        privateKey: normalizedPrivateKey,",
                ),
                (
                    "  generateKeyPair(privateKeyHex?: string, networkId?: string): { privateKey: string; publicKey: string; address: string } {\n    const kaspa = this.getKaspa();",
                    "  private normalizePrivateKeyInput(input: string): string {\n    let key = (input || \"\").trim();\n\n    if ((key.startsWith('\"') && key.endsWith('\"')) || (key.startsWith(\"'\") && key.endsWith(\"'\"))) {\n      key = key.slice(1, -1).trim();\n    }\n\n    if (key.startsWith(\"0x\") || key.startsWith(\"0X\")) {\n      key = key.slice(2).trim();\n    }\n\n    return key;\n  }\n\n  generateKeyPair(privateKeyHex?: string, networkId?: string): { privateKey: string; publicKey: string; address: string } {\n    const kaspa = this.getKaspa();",
                ),
            ],
        )?;
    }

    let auth_context = k_root.join("src").join("contexts").join("AuthContext.tsx");
    if auth_context.exists() {
        patch_file_replace_all(
            &auth_context,
            &[
                (
                    "      // Encrypt the private key with the password\n      const encrypted = CryptoJS.AES.encrypt(privateKeyInput, password).toString();",
                    "      // Encrypt normalized private key with the password\n      const encrypted = CryptoJS.AES.encrypt(keyPair.privateKey, password).toString();",
                ),
                (
                    "      throw new Error('Invalid private key or encryption failed');",
                    "      const reason = error instanceof Error ? error.message : 'unknown error';\n      throw new Error(`Invalid private key or encryption failed (${reason})`);",
                ),
            ],
        )?;
    }

    Ok(())
}

fn patch_file_replace_all(
    path: &Path,
    replacements: &[(&str, &str)],
) -> Result<(), Box<dyn Error>> {
    let mut contents = std::fs::read_to_string(path)?;
    let mut changed = false;

    for (from, to) in replacements {
        if contents.contains(to) {
            continue;
        }
        if contents.contains(from) {
            contents = contents.replace(from, to);
            changed = true;
        }
    }

    if changed {
        std::fs::write(path, contents)?;
        println!(
            "cargo:warning=Applied kaspa-ng compatibility patch to {}",
            path.display()
        );
    }

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

fn build_cpu_miner_if_needed() -> Result<(), Box<dyn Error>> {
    let host = std::env::var("HOST").unwrap_or_default();
    let target = std::env::var("TARGET").unwrap_or_default();
    if !host.is_empty() && !target.is_empty() && host != target {
        println!("cargo:warning=Skipping cpu miner build (cross-compile: {host} -> {target})");
        return Ok(());
    }

    if target.contains("wasm32") {
        println!("cargo:warning=Skipping cpu miner build (wasm32 target)");
        return Ok(());
    }

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let repo_root = manifest_dir
        .parent()
        .ok_or("failed to resolve repo root")?
        .to_path_buf();
    let miner_root = repo_root.join("cpuminer");
    if !miner_root.exists() {
        return Ok(());
    }

    let miner_src = miner_root.join("src");
    let miner_toml = miner_root.join("Cargo.toml");

    println!("cargo:rerun-if-changed={}", miner_toml.display());
    println!("cargo:rerun-if-changed={}", miner_src.display());

    let bin_name = if cfg!(windows) {
        "kaspa-miner.exe"
    } else {
        "kaspa-miner"
    };
    let bin_path = miner_root.join("target").join("release").join(bin_name);

    let latest_src = newest_mtime(&miner_src)
        .into_iter()
        .chain(newest_mtime(&miner_toml))
        .max();

    if bin_path.exists()
        && let (Some(bin_time), Some(src_time)) = (mtime(&bin_path), latest_src)
        && bin_time >= src_time
    {
        sync_cpu_miner_binary(&bin_path, &repo_root)?;
        return Ok(());
    }

    println!("cargo:warning=Building kaspa-miner (release)...");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = Command::new(cargo)
        .current_dir(&miner_root)
        .args(["build", "--release"])
        .status()?;

    if !status.success() {
        return Err("failed to build kaspa-miner".into());
    }

    sync_cpu_miner_binary(&bin_path, &repo_root)?;

    Ok(())
}

fn build_rothschild_if_needed() -> Result<(), Box<dyn Error>> {
    let host = std::env::var("HOST").unwrap_or_default();
    let target = std::env::var("TARGET").unwrap_or_default();
    if !host.is_empty() && !target.is_empty() && host != target {
        println!("cargo:warning=Skipping rothschild build (cross-compile: {host} -> {target})");
        return Ok(());
    }

    if target.contains("wasm32") {
        println!("cargo:warning=Skipping rothschild build (wasm32 target)");
        return Ok(());
    }

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let repo_root = manifest_dir
        .parent()
        .ok_or("failed to resolve repo root")?
        .to_path_buf();
    let rusty_kaspa = repo_root.join("rusty-kaspa");
    let rothschild_root = rusty_kaspa.join("rothschild");
    if !rothschild_root.exists() {
        return Ok(());
    }

    let rothschild_src = rothschild_root.join("src");
    let rothschild_toml = rothschild_root.join("Cargo.toml");
    let rusty_toml = rusty_kaspa.join("Cargo.toml");

    println!("cargo:rerun-if-changed={}", rothschild_toml.display());
    println!("cargo:rerun-if-changed={}", rusty_toml.display());
    println!("cargo:rerun-if-changed={}", rothschild_src.display());

    let bin_name = if cfg!(windows) {
        "rothschild.exe"
    } else {
        "rothschild"
    };
    let bin_path = rusty_kaspa.join("target").join("release").join(bin_name);

    let latest_src = newest_mtime(&rothschild_src)
        .into_iter()
        .chain(newest_mtime(&rothschild_toml))
        .chain(newest_mtime(&rusty_toml))
        .max();

    if bin_path.exists()
        && let (Some(bin_time), Some(src_time)) = (mtime(&bin_path), latest_src)
        && bin_time >= src_time
    {
        sync_rothschild_binary(&bin_path, &repo_root)?;
        return Ok(());
    }

    println!("cargo:warning=Building rothschild (release)...");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = Command::new(cargo)
        .current_dir(&rusty_kaspa)
        .args(["build", "-p", "rothschild", "--release"])
        .status()?;

    if !status.success() {
        return Err("failed to build rothschild".into());
    }

    sync_rothschild_binary(&bin_path, &repo_root)?;

    Ok(())
}

fn build_simply_kaspa_indexer_if_needed() -> Result<(), Box<dyn Error>> {
    let host = std::env::var("HOST").unwrap_or_default();
    let target = std::env::var("TARGET").unwrap_or_default();
    if !host.is_empty() && !target.is_empty() && host != target {
        println!(
            "cargo:warning=Skipping simply-kaspa-indexer build (cross-compile: {host} -> {target})"
        );
        return Ok(());
    }

    if target.contains("wasm32") {
        println!("cargo:warning=Skipping simply-kaspa-indexer build (wasm32 target)");
        return Ok(());
    }

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let repo_root = manifest_dir
        .parent()
        .ok_or("failed to resolve repo root")?
        .to_path_buf();
    let indexer_root = repo_root.join("simply-kaspa-indexer");
    if !indexer_root.exists() {
        return Ok(());
    }

    let indexer_toml = indexer_root.join("Cargo.toml");
    let indexer_lock = indexer_root.join("Cargo.lock");
    let cli_src = indexer_root.join("cli").join("src");
    let database_src = indexer_root.join("database").join("src");
    let mapping_src = indexer_root.join("mapping").join("src");
    let kaspad_src = indexer_root.join("kaspad").join("src");
    let indexer_src = indexer_root.join("indexer").join("src");
    let signal_src = indexer_root.join("signal").join("src");

    println!("cargo:rerun-if-changed={}", indexer_toml.display());
    println!("cargo:rerun-if-changed={}", indexer_lock.display());
    println!("cargo:rerun-if-changed={}", cli_src.display());
    println!("cargo:rerun-if-changed={}", database_src.display());
    println!("cargo:rerun-if-changed={}", mapping_src.display());
    println!("cargo:rerun-if-changed={}", kaspad_src.display());
    println!("cargo:rerun-if-changed={}", indexer_src.display());
    println!("cargo:rerun-if-changed={}", signal_src.display());

    let bin_name = if cfg!(windows) {
        "simply-kaspa-indexer.exe"
    } else {
        "simply-kaspa-indexer"
    };
    let bin_path = indexer_root.join("target").join("release").join(bin_name);

    let latest_src = newest_mtime(&indexer_toml)
        .into_iter()
        .chain(newest_mtime(&indexer_lock))
        .chain(newest_mtime(&cli_src))
        .chain(newest_mtime(&database_src))
        .chain(newest_mtime(&mapping_src))
        .chain(newest_mtime(&kaspad_src))
        .chain(newest_mtime(&indexer_src))
        .chain(newest_mtime(&signal_src))
        .max();

    if bin_path.exists()
        && let (Some(bin_time), Some(src_time)) = (mtime(&bin_path), latest_src)
        && bin_time >= src_time
    {
        sync_simply_kaspa_indexer_binary(&bin_path, &repo_root)?;
        return Ok(());
    }

    println!("cargo:warning=Building simply-kaspa-indexer (release)...");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = Command::new(cargo)
        .current_dir(&indexer_root)
        .args(["build", "-p", "simply-kaspa-indexer", "--release"])
        .status()?;

    if !status.success() {
        return Err("failed to build simply-kaspa-indexer".into());
    }

    sync_simply_kaspa_indexer_binary(&bin_path, &repo_root)?;

    Ok(())
}

fn build_k_indexer_if_needed() -> Result<(), Box<dyn Error>> {
    let host = std::env::var("HOST").unwrap_or_default();
    let target = std::env::var("TARGET").unwrap_or_default();
    if !host.is_empty() && !target.is_empty() && host != target {
        println!("cargo:warning=Skipping K-indexer build (cross-compile: {host} -> {target})");
        return Ok(());
    }

    if target.contains("wasm32") {
        println!("cargo:warning=Skipping K-indexer build (wasm32 target)");
        return Ok(());
    }

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let repo_root = manifest_dir
        .parent()
        .ok_or("failed to resolve repo root")?
        .to_path_buf();
    let k_indexer_root = repo_root.join("K-indexer");
    if !k_indexer_root.exists() {
        return Ok(());
    }

    let k_indexer_toml = k_indexer_root.join("Cargo.toml");
    let web_src = k_indexer_root.join("K-webserver").join("src");
    let web_toml = k_indexer_root.join("K-webserver").join("Cargo.toml");
    let processor_src = k_indexer_root.join("K-transaction-processor").join("src");
    let processor_toml = k_indexer_root
        .join("K-transaction-processor")
        .join("Cargo.toml");

    println!("cargo:rerun-if-changed={}", k_indexer_toml.display());
    println!("cargo:rerun-if-changed={}", web_toml.display());
    println!("cargo:rerun-if-changed={}", web_src.display());
    println!("cargo:rerun-if-changed={}", processor_toml.display());
    println!("cargo:rerun-if-changed={}", processor_src.display());

    let web_bin_name = if cfg!(windows) {
        "K-webserver.exe"
    } else {
        "K-webserver"
    };
    let processor_bin_name = if cfg!(windows) {
        "K-transaction-processor.exe"
    } else {
        "K-transaction-processor"
    };

    let web_bin = k_indexer_root
        .join("target")
        .join("release")
        .join(web_bin_name);
    let processor_bin = k_indexer_root
        .join("target")
        .join("release")
        .join(processor_bin_name);

    let latest_src = newest_mtime(&k_indexer_toml)
        .into_iter()
        .chain(newest_mtime(&web_toml))
        .chain(newest_mtime(&web_src))
        .chain(newest_mtime(&processor_toml))
        .chain(newest_mtime(&processor_src))
        .max();

    if web_bin.exists()
        && processor_bin.exists()
        && let Some(src_time) = latest_src
        && mtime(&web_bin)
            .map(|time| time >= src_time)
            .unwrap_or(false)
        && mtime(&processor_bin)
            .map(|time| time >= src_time)
            .unwrap_or(false)
    {
        sync_k_indexer_binaries(&web_bin, &processor_bin, &repo_root)?;
        return Ok(());
    }

    println!("cargo:warning=Building K-indexer components (release)...");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = Command::new(cargo)
        .current_dir(&k_indexer_root)
        .args([
            "build",
            "-p",
            "K-webserver",
            "-p",
            "K-transaction-processor",
            "--release",
        ])
        .status()?;

    if !status.success() {
        return Err("failed to build K-indexer components".into());
    }

    sync_k_indexer_binaries(&web_bin, &processor_bin, &repo_root)?;
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

fn sync_k_social_build(k_root: &Path, repo_root: &Path) -> Result<(), Box<dyn Error>> {
    let src_root = k_root.join("dist");
    if !src_root.join("index.html").exists() {
        return Ok(());
    }

    let dest_root = target_profile_dir(repo_root).join("K").join("dist");
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

fn sync_cpu_miner_binary(bin_path: &Path, repo_root: &Path) -> Result<(), Box<dyn Error>> {
    if !bin_path.exists() {
        return Ok(());
    }

    let target_dir = target_profile_dir(repo_root);
    std::fs::create_dir_all(&target_dir)?;
    let dest_path = target_dir.join(
        bin_path
            .file_name()
            .ok_or("failed to resolve cpu miner filename")?,
    );

    let src_time = mtime(bin_path);
    let dest_time = mtime(&dest_path);
    if dest_path.exists() && dest_time.is_some() && src_time.is_some() && dest_time >= src_time {
        return Ok(());
    }

    std::fs::copy(bin_path, dest_path)?;
    Ok(())
}

fn sync_rothschild_binary(bin_path: &Path, repo_root: &Path) -> Result<(), Box<dyn Error>> {
    if !bin_path.exists() {
        return Ok(());
    }

    let target_dir = target_profile_dir(repo_root);
    std::fs::create_dir_all(&target_dir)?;
    let dest_path = target_dir.join(
        bin_path
            .file_name()
            .ok_or("failed to resolve rothschild filename")?,
    );

    let src_time = mtime(bin_path);
    let dest_time = mtime(&dest_path);
    if dest_path.exists() && dest_time.is_some() && src_time.is_some() && dest_time >= src_time {
        return Ok(());
    }

    std::fs::copy(bin_path, dest_path)?;
    Ok(())
}

fn sync_simply_kaspa_indexer_binary(
    bin_path: &Path,
    repo_root: &Path,
) -> Result<(), Box<dyn Error>> {
    if !bin_path.exists() {
        return Ok(());
    }

    let target_dir = target_profile_dir(repo_root);
    std::fs::create_dir_all(&target_dir)?;
    let dest_path = target_dir.join(
        bin_path
            .file_name()
            .ok_or("failed to resolve simply-kaspa-indexer filename")?,
    );

    let src_time = mtime(bin_path);
    let dest_time = mtime(&dest_path);
    if dest_path.exists() && dest_time.is_some() && src_time.is_some() && dest_time >= src_time {
        return Ok(());
    }

    std::fs::copy(bin_path, dest_path)?;
    Ok(())
}

fn sync_k_indexer_binaries(
    web_bin: &Path,
    processor_bin: &Path,
    repo_root: &Path,
) -> Result<(), Box<dyn Error>> {
    if !web_bin.exists() || !processor_bin.exists() {
        return Ok(());
    }

    let target_dir = target_profile_dir(repo_root);
    std::fs::create_dir_all(&target_dir)?;

    for bin_path in [web_bin, processor_bin] {
        let dest_path = target_dir.join(
            bin_path
                .file_name()
                .ok_or("failed to resolve K-indexer binary filename")?,
        );

        let src_time = mtime(bin_path);
        let dest_time = mtime(&dest_path);
        if dest_path.exists() && dest_time.is_some() && src_time.is_some() && dest_time >= src_time
        {
            continue;
        }

        std::fs::copy(bin_path, dest_path)?;
    }

    Ok(())
}

fn target_profile_dir(repo_root: &Path) -> PathBuf {
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "release".to_string());
    if let Ok(target_dir) = std::env::var("CARGO_TARGET_DIR") {
        return PathBuf::from(target_dir).join(profile);
    }
    let mut target_dir = repo_root.join("target");
    if let Ok(build_target) = std::env::var("CARGO_BUILD_TARGET")
        && !build_target.is_empty()
    {
        target_dir = target_dir.join(build_target);
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

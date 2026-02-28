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
    sync_external_repo_if_needed("Kasia", "https://github.com/K-Kluster/Kasia.git")?;
    sync_external_repo_if_needed("kasvault", "https://github.com/coderofstuff/kasvault.git")?;
    sync_external_repo_if_needed(
        "kasia-indexer",
        "https://github.com/K-Kluster/kasia-indexer.git",
    )?;
    build_explorer_if_needed()?;
    if external_builds_enabled() {
        build_k_social_if_needed()?;
        build_kasia_if_needed()?;
        build_kasvault_if_needed()?;
    }
    build_cpu_miner_if_needed()?;
    build_rothschild_if_needed()?;
    build_simply_kaspa_indexer_if_needed()?;
    if external_builds_enabled() {
        build_k_indexer_if_needed()?;
        build_kasia_indexer_if_needed()?;
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
    if std::env::var("CI")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
        && !std::env::var("KASPA_NG_FORCE_EXTERNAL_SYNC")
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    {
        println!("cargo:warning=Skipping sync for {name} in CI");
        return Ok(());
    }

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
        println!("cargo:warning={name} has no .git; bootstrapping git metadata");
        if !bootstrap_git_metadata(&repo_root, &target, name, url)? {
            println!("cargo:warning=Skipping sync for {name} (not a git repository)");
            return Ok(());
        }
        // Freshly bootstrapped metadata can report the current directory contents as local
        // changes depending on checkout provenance. Defer pull/stash to the next build to
        // avoid noisy stash/apply conflicts in CI and preserve local edits safely.
        println!("cargo:warning=Skipping pull for {name} on first metadata bootstrap");
        return Ok(());
    }

    let mut created_stash_ref: Option<String> = None;
    if has_local_git_changes(&target)? {
        println!("cargo:warning={name} has local changes; stashing before pull");
        let before_stash = stash_head_oid(&target)?;
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
            let after_stash = stash_head_oid(&target)?;
            if after_stash != before_stash {
                created_stash_ref = Some("stash@{0}".to_string());
            } else {
                println!(
                    "cargo:warning={name} stash command succeeded but no stash entry was created"
                );
                println!(
                    "cargo:warning=Skipping pull for {name} to avoid overwriting unstashed local changes"
                );
                return Ok(());
            }
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

    if let Some(stash_ref) = created_stash_ref {
        let apply_with_index = Command::new("git")
            .current_dir(&target)
            .args(["stash", "apply", "--index", &stash_ref])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        let reapplied = if apply_with_index {
            true
        } else {
            println!(
                "cargo:warning=Failed to re-apply stashed local changes with --index for {name}; retrying without index"
            );
            Command::new("git")
                .current_dir(&target)
                .args(["stash", "apply", &stash_ref])
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        };

        if reapplied {
            let dropped = Command::new("git")
                .current_dir(&target)
                .args(["stash", "drop", &stash_ref])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !dropped {
                println!(
                    "cargo:warning=Re-applied local stash for {name}, but failed to drop {stash_ref}"
                );
            }
        } else {
            println!(
                "cargo:warning=Failed to re-apply stashed local changes for {name}; stash kept as {stash_ref}"
            );
        }
    }

    Ok(())
}

fn bootstrap_git_metadata(
    repo_root: &Path,
    target: &Path,
    name: &str,
    url: &str,
) -> Result<bool, Box<dyn Error>> {
    let target_git = target.join(".git");
    if target_git.exists() {
        return Ok(true);
    }

    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp_name = format!(".kaspa-ng-sync-{name}-{pid}-{nanos}");
    let tmp_clone = repo_root.join(tmp_name);

    let cloned = Command::new("git")
        .current_dir(repo_root)
        .args(["clone", "--depth", "1", "--no-checkout", url])
        .arg(&tmp_clone)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !cloned {
        let _ = std::fs::remove_dir_all(&tmp_clone);
        return Ok(false);
    }

    let tmp_git = tmp_clone.join(".git");
    if !tmp_git.exists() {
        let _ = std::fs::remove_dir_all(&tmp_clone);
        return Ok(false);
    }

    let moved = std::fs::rename(&tmp_git, &target_git).is_ok();
    let _ = std::fs::remove_dir_all(&tmp_clone);
    if !moved {
        return Ok(false);
    }

    let ok = Command::new("git")
        .current_dir(target)
        .args(["rev-parse", "--is-inside-work-tree"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        let _ = std::fs::remove_dir_all(&target_git);
        return Ok(false);
    }

    println!(
        "cargo:warning={name} git metadata bootstrapped; local changes will be stashed before pull"
    );
    Ok(true)
}

fn has_local_git_changes(repo_dir: &Path) -> Result<bool, Box<dyn Error>> {
    let output = Command::new("git")
        .current_dir(repo_dir)
        .args(["status", "--porcelain"])
        .output()?;
    Ok(!output.stdout.is_empty())
}

fn stash_head_oid(repo_dir: &Path) -> Result<Option<String>, Box<dyn Error>> {
    let output = Command::new("git")
        .current_dir(repo_dir)
        .args(["rev-parse", "-q", "--verify", "refs/stash"])
        .output()?;
    if !output.status.success() {
        return Ok(None);
    }
    let line = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if line.is_empty() {
        Ok(None)
    } else {
        Ok(Some(line))
    }
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

fn build_kasia_if_needed() -> Result<(), Box<dyn Error>> {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let repo_root = manifest_dir
        .parent()
        .ok_or("failed to resolve repo root")?
        .to_path_buf();
    let kasia_root = repo_root.join("Kasia");
    if !kasia_root.exists() {
        return Ok(());
    }

    let package_json = kasia_root.join("package.json");
    let lockfile = kasia_root.join("package-lock.json");
    let src_dir = kasia_root.join("src");
    let public_dir = kasia_root.join("public");
    let scripts_dir = kasia_root.join("scripts");
    let cipher_dir = kasia_root.join("cipher");
    let wasm_dir = kasia_root.join("wasm");
    let wasm_package_json = wasm_dir.join("package.json");
    let biometry_vendor_dir = kasia_root.join("vendors").join("tauri-plugin-biometry");
    let biometry_vendor_package = biometry_vendor_dir.join("package.json");
    let build_index = kasia_root.join("dist").join("index.html");

    println!("cargo:rerun-if-changed={}", package_json.display());
    println!("cargo:rerun-if-changed={}", lockfile.display());
    println!("cargo:rerun-if-changed={}", src_dir.display());
    println!("cargo:rerun-if-changed={}", public_dir.display());
    println!("cargo:rerun-if-changed={}", scripts_dir.display());
    println!("cargo:rerun-if-changed={}", cipher_dir.display());
    println!("cargo:rerun-if-changed={}", wasm_dir.display());
    println!("cargo:rerun-if-env-changed=KASIA_WASM_SDK_URL");
    println!("cargo:rerun-if-env-changed=KASIA_WASM_AUTO_FETCH");
    println!(
        "cargo:rerun-if-changed={}",
        kasia_root.join("vite.config.ts").display()
    );

    apply_kasia_runtime_patches(&kasia_root)?;
    ensure_kasia_biometry_stub(&biometry_vendor_dir)?;

    let latest_src = newest_mtime(&package_json)
        .into_iter()
        .chain(newest_mtime(&lockfile))
        .chain(newest_mtime(&src_dir))
        .chain(newest_mtime(&public_dir))
        .chain(newest_mtime(&scripts_dir))
        .chain(newest_mtime(&cipher_dir))
        .chain(newest_mtime(&wasm_dir))
        .max();

    if build_index.exists()
        && let (Some(bin_time), Some(src_time)) = (mtime(&build_index), latest_src)
        && bin_time >= src_time
    {
        sync_kasia_build(&kasia_root, &repo_root)?;
        return Ok(());
    }

    println!("cargo:warning=Building Kasia (static)...");
    let npm = std::env::var("NPM").unwrap_or_else(|_| "npm".to_string());
    let node_modules = kasia_root.join("node_modules");
    let npm_cmd = |args: &[&str]| {
        let mut cmd = Command::new(&npm);
        cmd.current_dir(&kasia_root)
            .env("HUSKY", "0")
            .env("CI", "true")
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
            println!("cargo:warning=Kasia npm install failed; skipping build");
            return Ok(());
        }
    }

    if !biometry_vendor_package.exists() {
        println!(
            "cargo:warning=Kasia biometry package is missing; creating no-op local plugin stub"
        );
        ensure_kasia_biometry_stub(&biometry_vendor_dir)?;
    }

    if !wasm_package_json.exists() {
        ensure_kasia_wasm_package(&repo_root, &kasia_root, &wasm_dir, &wasm_package_json)?;
    }

    if !wasm_package_json.exists() {
        println!(
            "cargo:warning=Kasia wasm package missing at {}; skipping Kasia rebuild (see Kasia/README.md setup)",
            wasm_package_json.display()
        );
        sync_kasia_build(&kasia_root, &repo_root)?;
        return Ok(());
    }

    let wasm_status = npm_cmd(&["run", "wasm:build"]).status();
    if wasm_status.map(|s| !s.success()).unwrap_or(true) {
        println!("cargo:warning=Kasia wasm:build failed; continuing with production build");
    }

    let status = npm_cmd(&["run", "build:production"]).status();
    if status.map(|s| !s.success()).unwrap_or(true) {
        println!("cargo:warning=Kasia build:production failed; trying vite build fallback");
        let fallback = npm_cmd(&["exec", "vite", "build"]).status();
        if fallback.map(|s| !s.success()).unwrap_or(true) {
            println!("cargo:warning=Kasia build failed; skipping");
        }
    }

    sync_kasia_build(&kasia_root, &repo_root)?;
    Ok(())
}

fn build_kasvault_if_needed() -> Result<(), Box<dyn Error>> {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let repo_root = manifest_dir
        .parent()
        .ok_or("failed to resolve repo root")?
        .to_path_buf();
    let kasvault_root = repo_root.join("kasvault");
    if !kasvault_root.exists() {
        return Ok(());
    }

    let package_json = kasvault_root.join("package.json");
    let lockfile = kasvault_root.join("package-lock.json");
    let src_dir = kasvault_root.join("src");
    let public_dir = kasvault_root.join("public");
    let config_overrides = kasvault_root.join("config-overrides.js");
    let build_index = kasvault_root.join("build").join("index.html");

    println!("cargo:rerun-if-changed={}", package_json.display());
    println!("cargo:rerun-if-changed={}", lockfile.display());
    println!("cargo:rerun-if-changed={}", src_dir.display());
    println!("cargo:rerun-if-changed={}", public_dir.display());
    println!("cargo:rerun-if-changed={}", config_overrides.display());

    let latest_src = newest_mtime(&package_json)
        .into_iter()
        .chain(newest_mtime(&lockfile))
        .chain(newest_mtime(&src_dir))
        .chain(newest_mtime(&public_dir))
        .chain(newest_mtime(&config_overrides))
        .max();

    if build_index.exists()
        && let (Some(bin_time), Some(src_time)) = (mtime(&build_index), latest_src)
        && bin_time >= src_time
    {
        sync_kasvault_build(&kasvault_root, &repo_root)?;
        return Ok(());
    }

    println!("cargo:warning=Building KasVault (static)...");
    let npm = std::env::var("NPM").unwrap_or_else(|_| "npm".to_string());
    let node_modules = kasvault_root.join("node_modules");

    if !node_modules.exists() {
        let status = if lockfile.exists() {
            Command::new(&npm)
                .current_dir(&kasvault_root)
                .args(["ci", "--no-audit", "--no-fund"])
                .status()
        } else {
            Command::new(&npm)
                .current_dir(&kasvault_root)
                .args(["install", "--no-audit", "--no-fund"])
                .status()
        };
        if status.map(|s| !s.success()).unwrap_or(true) {
            println!("cargo:warning=KasVault npm install failed; skipping build");
            return Ok(());
        }
    }

    let status = Command::new(&npm)
        .current_dir(&kasvault_root)
        .args(["run", "build"])
        .status();
    if status.map(|s| !s.success()).unwrap_or(true) {
        println!("cargo:warning=KasVault build failed; skipping");
    }

    sync_kasvault_build(&kasvault_root, &repo_root)?;
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

fn apply_kasia_runtime_patches(kasia_root: &Path) -> Result<(), Box<dyn Error>> {
    let init_ts = kasia_root.join("src").join("init.ts");
    if init_ts.exists() {
        patch_file_replace_all(
            &init_ts,
            &[
                (
                    "import initKaspaWasm, { initConsolePanicHook } from \"kaspa-wasm\";",
                    "import * as kaspaWasm from \"kaspa-wasm\";",
                ),
                (
                    "let splashElement: HTMLElement;\n",
                    "let splashElement: HTMLElement;\n\ntype KaspaInitModule = {\n  default?: () => Promise<unknown>;\n  init?: () => Promise<unknown>;\n  initSync?: (...args: unknown[]) => unknown;\n  initConsolePanicHook?: () => void;\n};\n\nasync function initKaspaWasmCompat() {\n  const mod = kaspaWasm as unknown as KaspaInitModule;\n\n  if (typeof mod.default === \"function\") {\n    await mod.default();\n    return;\n  }\n\n  if (typeof mod.init === \"function\") {\n    await mod.init();\n    return;\n  }\n\n  throw new Error(\n    \"kaspa-wasm init function not found (expected default export or init())\"\n  );\n}\n",
                ),
                (
                    "  await Promise.all([initKaspaWasm(), initCipherWasm()]);",
                    "  await Promise.all([initKaspaWasmCompat(), initCipherWasm()]);",
                ),
                (
                    "  initConsolePanicHook();",
                    "  const panicHook = (kaspaWasm as unknown as KaspaInitModule)\n    .initConsolePanicHook;\n  if (typeof panicHook === \"function\") {\n    panicHook();\n  }",
                ),
                (
                    "  indexerClient.setConfig({\n    baseUrl:\n      import.meta.env.VITE_DEFAULT_KASPA_NETWORK === \"mainnet\"\n        ? import.meta.env.VITE_INDEXER_MAINNET_URL\n        : import.meta.env.VITE_INDEXER_TESTNET_URL,\n  });",
                    "  const runtimeConfig =\n    (globalThis as { __KASPA_NG_KASIA_CONFIG?: { indexerMainnetUrl?: string; indexerTestnetUrl?: string } })\n      .__KASPA_NG_KASIA_CONFIG ?? {};\n\n  indexerClient.setConfig({\n    baseUrl:\n      import.meta.env.VITE_DEFAULT_KASPA_NETWORK === \"mainnet\"\n        ? runtimeConfig.indexerMainnetUrl ?? import.meta.env.VITE_INDEXER_MAINNET_URL\n        : runtimeConfig.indexerTestnetUrl ?? import.meta.env.VITE_INDEXER_TESTNET_URL,\n  });",
                ),
            ],
        )?;
    }

    let orchestrator = kasia_root
        .join("src")
        .join("hooks")
        .join("useOrchestrator.ts");
    if orchestrator.exists() {
        patch_file_replace_all(
            &orchestrator,
            &[(
                "    // update indexer base url\n    indexerClient.setConfig({\n      baseUrl:\n        (opts?.networkType ?? baseNetwork) === \"mainnet\"\n          ? import.meta.env.VITE_INDEXER_MAINNET_URL\n          : import.meta.env.VITE_INDEXER_TESTNET_URL,\n    });",
                "    const runtimeConfig =\n      (globalThis as { __KASPA_NG_KASIA_CONFIG?: { indexerMainnetUrl?: string; indexerTestnetUrl?: string } })\n        .__KASPA_NG_KASIA_CONFIG ?? {};\n\n    // update indexer base url\n    indexerClient.setConfig({\n      baseUrl:\n        (opts?.networkType ?? baseNetwork) === \"mainnet\"\n          ? runtimeConfig.indexerMainnetUrl ?? import.meta.env.VITE_INDEXER_MAINNET_URL\n          : runtimeConfig.indexerTestnetUrl ?? import.meta.env.VITE_INDEXER_TESTNET_URL,\n    });",
            )],
        )?;
    }

    let network_store = kasia_root
        .join("src")
        .join("store")
        .join("network.store.ts");
    if network_store.exists() {
        patch_file_replace_all(
            &network_store,
            &[(
                "      const kasiaNodeUrl =\n        rpc.networkId?.toString() === \"mainnet\"\n          ? import.meta.env.VITE_DEFAULT_MAINNET_KASPA_NODE_URL\n          : import.meta.env.VITE_DEFAULT_TESTNET_KASPA_NODE_URL;",
                "      const runtimeConfig =\n        (globalThis as {\n          __KASPA_NG_KASIA_CONFIG?: {\n            defaultMainnetNodeUrl?: string;\n            defaultTestnetNodeUrl?: string;\n          };\n        }).__KASPA_NG_KASIA_CONFIG ?? {};\n\n      const kasiaNodeUrl =\n        rpc.networkId?.toString() === \"mainnet\"\n          ? runtimeConfig.defaultMainnetNodeUrl ??\n            import.meta.env.VITE_DEFAULT_MAINNET_KASPA_NODE_URL\n          : runtimeConfig.defaultTestnetNodeUrl ??\n            import.meta.env.VITE_DEFAULT_TESTNET_KASPA_NODE_URL;",
            )],
        )?;
    }

    let session_store = kasia_root
        .join("src")
        .join("store")
        .join("session.store.ts");
    if session_store.exists() {
        patch_file_replace_all(
            &session_store,
            &[
                (
                    "import {\n  hasData,\n  setData,\n  getData,\n  checkStatus,\n} from \"@tauri-apps/plugin-biometry\";\n",
                    "",
                ),
                (
                    "    async supportSecuredBiometry() {\n      try {\n        if (!core.isTauri()) {\n          return false;\n        }\n\n        // temporary disable for iOS\n        if (platform() === \"ios\") {\n          return false;\n        }\n\n        const status = await checkStatus();\n\n        return status.isAvailable;\n      } catch (error) {\n        console.error(error);\n        return false;\n      }\n    },",
                    "    async supportSecuredBiometry() {\n      return false;\n    },",
                ),
                (
                    "    async getSession(tenantId) {\n      // safeguard is case of mis-use in browser context\n      if (!core.isTauri()) {\n        return null;\n      }\n\n      // temporary disable for iOS\n      if (platform() === \"ios\") {\n        return null;\n      }\n\n      const data = await getData({\n        domain: \"kas.kluster.kasia\",\n        name: `${tenantId}.password`,\n        reason: \"Access your messages\",\n        cancelTitle: \"Use Password\",\n      });\n\n      return data?.data ?? null;\n    },",
                    "    async getSession(_tenantId) {\n      return null;\n    },",
                ),
                (
                    "    async hasSession(tenantId) {\n      // temporary disable for iOS\n      if (!core.isTauri() || platform() === \"ios\") {\n        return false;\n      }\n\n      if (!(await get().supportSecuredBiometry())) {\n        return false;\n      }\n\n      return hasData({\n        domain: \"kas.kluster.kasia\",\n        name: `${tenantId}.password`,\n      });\n    },",
                    "    async hasSession(_tenantId) {\n      return false;\n    },",
                ),
                (
                    "    async setSession(tenantId, password) {\n      // temporary disable for iOS\n      if (!core.isTauri() || platform() === \"ios\") {\n        return;\n      }\n\n      if (!(await get().supportSecuredBiometry())) {\n        return;\n      }\n\n      await setData({\n        data: password,\n        domain: \"kas.kluster.kasia\",\n        name: `${tenantId}.password`,\n      });\n    },",
                    "    async setSession(_tenantId, _password) {\n      return;\n    },",
                ),
            ],
        )?;
    }

    let resizable_container = kasia_root
        .join("src")
        .join("components")
        .join("Layout")
        .join("ResizableAppContainer.tsx");
    if resizable_container.exists() {
        patch_file_replace_all(
            &resizable_container,
            &[
                (
                    "const [width, setWidth] = useState<number>(CONTAINER_DEFAULT); // set default to 1600",
                    "const [width, setWidth] = useState<number>(window.innerWidth);",
                ),
                (
                    "    const handleResize = () => {\n      setWindowWidth(window.innerWidth);\n    };",
                    "    const handleResize = () => {\n      setWindowWidth(window.innerWidth);\n      setWidth(window.innerWidth);\n    };",
                ),
                (
                    "        isMobile ? \"w-full\" : \"relative mx-auto rounded-lg shadow-2xl\"",
                    "        isMobile ? \"w-full\" : \"relative w-full\"",
                ),
                (
                    "              width,",
                    "              width: Math.min(width, windowWidth),",
                ),
            ],
        )?;
    }

    Ok(())
}

fn ensure_kasia_biometry_stub(vendor_dir: &Path) -> Result<(), Box<dyn Error>> {
    std::fs::create_dir_all(vendor_dir)?;
    let package_json = vendor_dir.join("package.json");
    if !package_json.exists() {
        std::fs::write(
            &package_json,
            "{\n  \"name\": \"@tauri-apps/plugin-biometry\",\n  \"version\": \"0.0.0-kaspa-ng-stub\",\n  \"type\": \"module\",\n  \"main\": \"index.js\",\n  \"types\": \"index.d.ts\"\n}\n",
        )?;
    }

    let index_js = vendor_dir.join("index.js");
    if !index_js.exists() {
        std::fs::write(
            &index_js,
            "export async function checkStatus() { return { isAvailable: false }; }\nexport async function hasData() { return false; }\nexport async function getData() { return null; }\nexport async function setData() { return; }\n",
        )?;
    }

    let index_d_ts = vendor_dir.join("index.d.ts");
    if !index_d_ts.exists() {
        std::fs::write(
            &index_d_ts,
            "export declare function checkStatus(): Promise<{ isAvailable: boolean }>;\nexport declare function hasData(_args: { domain: string; name: string }): Promise<boolean>;\nexport declare function getData(_args: { domain: string; name: string; reason?: string; cancelTitle?: string }): Promise<{ data: string } | null>;\nexport declare function setData(_args: { domain: string; name: string; data: string }): Promise<void>;\n",
        )?;
    }

    Ok(())
}

fn ensure_kasia_wasm_package(
    repo_root: &Path,
    _kasia_root: &Path,
    wasm_dir: &Path,
    wasm_package_json: &Path,
) -> Result<(), Box<dyn Error>> {
    if wasm_package_json.exists() {
        return Ok(());
    }

    let auto_fetch = std::env::var("KASIA_WASM_AUTO_FETCH")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(true);
    if !auto_fetch {
        println!("cargo:warning=Kasia wasm auto-fetch disabled (KASIA_WASM_AUTO_FETCH=0)");
        return Ok(());
    }

    let urls = if let Ok(url) = std::env::var("KASIA_WASM_SDK_URL") {
        vec![url]
    } else {
        vec![
            "https://github.com/IzioDev/rusty-kaspa/releases/download/v1.0.1-beta1/kaspa-wasm32-sdk-v1.0.1-beta1.zip".to_string(),
            "https://github.com/kaspanet/rusty-kaspa/releases/download/v1.0.0/kaspa-wasm32-sdk-v1.0.0.zip".to_string(),
        ]
    };

    let tmp_root = repo_root.join("target").join("kasia-wasm-fetch");
    if tmp_root.exists() {
        let _ = std::fs::remove_dir_all(&tmp_root);
    }
    std::fs::create_dir_all(&tmp_root)?;

    for (idx, url) in urls.iter().enumerate() {
        println!(
            "cargo:warning=Attempting Kasia wasm SDK download ({}) from {url}",
            idx + 1
        );

        let archive_path = tmp_root.join(format!("sdk-{idx}.zip"));
        if !download_file(url, &archive_path) {
            println!("cargo:warning=Kasia wasm download failed from {url}");
            continue;
        }

        let extract_root = tmp_root.join(format!("extract-{idx}"));
        let _ = std::fs::remove_dir_all(&extract_root);
        std::fs::create_dir_all(&extract_root)?;
        if !extract_zip(&archive_path, &extract_root) {
            println!("cargo:warning=Failed to extract Kasia wasm archive from {url}");
            continue;
        }

        let Some(source_wasm_dir) = find_kasia_wasm_source_dir(&extract_root) else {
            println!(
                "cargo:warning=Kasia wasm archive does not contain expected web/kaspa package ({url})"
            );
            continue;
        };

        if wasm_dir.exists() {
            let _ = std::fs::remove_dir_all(wasm_dir);
        }
        std::fs::create_dir_all(wasm_dir)?;
        copy_dir_all(&source_wasm_dir, wasm_dir)?;

        if wasm_package_json.exists() {
            println!(
                "cargo:warning=Kasia wasm package restored from {url} into {}",
                wasm_dir.display()
            );
            return Ok(());
        }
    }

    println!(
        "cargo:warning=Unable to auto-fetch Kasia wasm package. Set KASIA_WASM_SDK_URL to a valid sdk zip."
    );
    Ok(())
}

fn download_file(url: &str, destination: &Path) -> bool {
    let status = Command::new("curl")
        .args(["-fL", "--retry", "2", "-o"])
        .arg(destination)
        .arg(url)
        .status();
    status.map(|s| s.success()).unwrap_or(false)
}

fn extract_zip(archive: &Path, destination: &Path) -> bool {
    let status = Command::new("unzip")
        .args(["-q"])
        .arg(archive)
        .args(["-d"])
        .arg(destination)
        .status();
    status.map(|s| s.success()).unwrap_or(false)
}

fn find_kasia_wasm_source_dir(root: &Path) -> Option<PathBuf> {
    let direct = root.join("kaspa-wasm32-sdk").join("web").join("kaspa");
    if direct.join("package.json").exists() {
        return Some(direct);
    }

    fn walk(dir: &Path, depth: usize) -> Option<PathBuf> {
        if depth > 6 {
            return None;
        }
        let entries = std::fs::read_dir(dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if path.file_name().and_then(|n| n.to_str()) == Some("kaspa")
                && path.join("package.json").exists()
            {
                return Some(path);
            }
            if let Some(found) = walk(&path, depth + 1) {
                return Some(found);
            }
        }
        None
    }

    walk(root, 0)
}

fn build_kasia_indexer_if_needed() -> Result<(), Box<dyn Error>> {
    let host = std::env::var("HOST").unwrap_or_default();
    let target = std::env::var("TARGET").unwrap_or_default();
    if !host.is_empty() && !target.is_empty() && host != target {
        println!("cargo:warning=Skipping kasia-indexer build (cross-compile: {host} -> {target})");
        return Ok(());
    }

    if target.contains("wasm32") {
        println!("cargo:warning=Skipping kasia-indexer build (wasm32 target)");
        return Ok(());
    }

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let repo_root = manifest_dir
        .parent()
        .ok_or("failed to resolve repo root")?
        .to_path_buf();
    let indexer_root = repo_root.join("kasia-indexer");
    if !indexer_root.exists() {
        return Ok(());
    }

    let indexer_toml = indexer_root.join("Cargo.toml");
    let indexer_lock = indexer_root.join("Cargo.lock");
    let indexer_src = indexer_root.join("indexer").join("src");
    let actors_src = indexer_root.join("indexer-actors").join("src");
    let db_src = indexer_root.join("indexer-db").join("src");
    let protocol_src = indexer_root.join("protocol").join("src");

    println!("cargo:rerun-if-changed={}", indexer_toml.display());
    println!("cargo:rerun-if-changed={}", indexer_lock.display());
    println!("cargo:rerun-if-changed={}", indexer_src.display());
    println!("cargo:rerun-if-changed={}", actors_src.display());
    println!("cargo:rerun-if-changed={}", db_src.display());
    println!("cargo:rerun-if-changed={}", protocol_src.display());

    let bin_name = if cfg!(windows) {
        "indexer.exe"
    } else {
        "indexer"
    };
    let bin_path = indexer_root.join("target").join("release").join(bin_name);

    let latest_src = newest_mtime(&indexer_toml)
        .into_iter()
        .chain(newest_mtime(&indexer_lock))
        .chain(newest_mtime(&indexer_src))
        .chain(newest_mtime(&actors_src))
        .chain(newest_mtime(&db_src))
        .chain(newest_mtime(&protocol_src))
        .max();

    if bin_path.exists()
        && let (Some(bin_time), Some(src_time)) = (mtime(&bin_path), latest_src)
        && bin_time >= src_time
    {
        sync_kasia_indexer_binary(&bin_path, &repo_root)?;
        return Ok(());
    }

    println!("cargo:warning=Building kasia-indexer (release)...");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = Command::new(cargo)
        .current_dir(&indexer_root)
        .args(["build", "-p", "indexer", "--release"])
        .status()?;

    if !status.success() {
        return Err("failed to build kasia-indexer".into());
    }

    sync_kasia_indexer_binary(&bin_path, &repo_root)?;
    Ok(())
}

fn patch_file_replace_all(
    path: &Path,
    replacements: &[(&str, &str)],
) -> Result<(), Box<dyn Error>> {
    let mut contents = std::fs::read_to_string(path)?;
    let mut changed = false;

    for (from, to) in replacements {
        let from_crlf = from.replace('\n', "\r\n");
        let to_crlf = to.replace('\n', "\r\n");

        if contents.contains(to) || contents.contains(&to_crlf) {
            continue;
        }
        if contents.contains(from) {
            contents = contents.replace(from, to);
            changed = true;
            continue;
        }
        if contents.contains(&from_crlf) {
            contents = contents.replace(&from_crlf, &to_crlf);
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
    apply_k_indexer_runtime_patches(&k_indexer_root)?;

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

fn apply_k_indexer_runtime_patches(k_indexer_root: &Path) -> Result<(), Box<dyn Error>> {
    let processor_toml = k_indexer_root
        .join("K-transaction-processor")
        .join("Cargo.toml");
    if processor_toml.exists() {
        replace_dependency_line(
            &processor_toml,
            "kaspa-wallet-core",
            "kaspa-hashes = { git = \"https://github.com/kaspanet/rusty-kaspa.git\" }",
        )?;
        patch_file_replace_all(
            &processor_toml,
            &[(
                "kaspa-wallet-core = { git = \"https://github.com/kaspanet/rusty-kaspa.git\", features = [\"wasm32-sdk\"] }",
                "kaspa-hashes = { git = \"https://github.com/kaspanet/rusty-kaspa.git\" }",
            )],
        )?;
    }

    let protocol_rs = k_indexer_root
        .join("K-transaction-processor")
        .join("src")
        .join("k_protocol.rs");
    if protocol_rs.exists() {
        patch_file_replace_all(
            &protocol_rs,
            &[
                (
                    "use kaspa_wallet_core::message::{PersonalMessage, verify_message};\nuse secp256k1::XOnlyPublicKey;",
                    "use kaspa_hashes::{Hash, PersonalMessageSigningHash};\nuse secp256k1::{Message, XOnlyPublicKey, schnorr::Signature};",
                ),
                (
                    "    /// Verify a Kaspa message signature using the proper kaspa-wallet-core verification\n    /// This uses Kaspa's PersonalMessageSigningHash and Schnorr signature verification\n    fn verify_kaspa_signature(&self, message: &str, signature: &str, public_key_hex: &str) -> bool {\n        // Create PersonalMessage from the message string\n        let personal_message = PersonalMessage(message);\n",
                    "    fn calc_personal_message_hash(message: &str) -> Hash {\n        let mut hasher = PersonalMessageSigningHash::new();\n        hasher.write(message.as_bytes());\n        hasher.finalize()\n    }\n\n    /// Verify a Kaspa message signature using Kaspa PersonalMessageSigningHash + Schnorr.\n    fn verify_kaspa_signature(&self, message: &str, signature: &str, public_key_hex: &str) -> bool {\n",
                ),
                (
                    "        // Verify the message signature using Kaspa's verify_message function\n        match verify_message(&personal_message, &signature_bytes, &public_key) {\n            Ok(()) => {\n                //info!(\"Kaspa message signature verification successful\");\n                true\n            }\n            Err(err) => {\n                error!(\"Kaspa message signature verification failed: {}\", err);\n                false\n            }\n        }\n",
                    "        let hash = Self::calc_personal_message_hash(message);\n        let msg = match Message::from_digest_slice(hash.as_bytes().as_slice()) {\n            Ok(msg) => msg,\n            Err(err) => {\n                error!(\"Failed to build secp256k1 message digest: {}\", err);\n                return false;\n            }\n        };\n        let sig = match Signature::from_slice(signature_bytes.as_slice()) {\n            Ok(sig) => sig,\n            Err(err) => {\n                error!(\"Failed to parse Schnorr signature: {}\", err);\n                return false;\n            }\n        };\n\n        let secp = secp256k1::Secp256k1::verification_only();\n        match secp.verify_schnorr(&sig, &msg, &public_key) {\n            Ok(()) => true,\n            Err(err) => {\n                error!(\"Kaspa message signature verification failed: {}\", err);\n                false\n            }\n        }\n",
                ),
            ],
        )?;
    }

    Ok(())
}

fn replace_dependency_line(
    path: &Path,
    dep_name: &str,
    replacement: &str,
) -> Result<(), Box<dyn Error>> {
    let contents = std::fs::read_to_string(path)?;
    let marker = format!("{dep_name} =");
    let mut changed = false;
    let mut rewritten = String::with_capacity(contents.len() + 64);

    for line in contents.lines() {
        if line.trim_start().starts_with(&marker) {
            rewritten.push_str(replacement);
            rewritten.push('\n');
            changed = true;
        } else {
            rewritten.push_str(line);
            rewritten.push('\n');
        }
    }

    if changed {
        std::fs::write(path, rewritten)?;
        println!(
            "cargo:warning=Applied kaspa-ng dependency patch to {}",
            path.display()
        );
    }

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

fn sync_kasia_build(kasia_root: &Path, repo_root: &Path) -> Result<(), Box<dyn Error>> {
    let src_root = kasia_root.join("dist");
    if !src_root.join("index.html").exists() {
        return Ok(());
    }

    let dest_root = target_profile_dir(repo_root).join("Kasia").join("dist");
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

fn sync_kasvault_build(kasvault_root: &Path, repo_root: &Path) -> Result<(), Box<dyn Error>> {
    let src_root = kasvault_root.join("build");
    if !src_root.join("index.html").exists() {
        return Ok(());
    }

    let dest_root = target_profile_dir(repo_root).join("KasVault").join("build");
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

fn sync_kasia_indexer_binary(bin_path: &Path, repo_root: &Path) -> Result<(), Box<dyn Error>> {
    if !bin_path.exists() {
        return Ok(());
    }

    let target_dir = target_profile_dir(repo_root);
    std::fs::create_dir_all(&target_dir)?;

    let dest_filename = if cfg!(windows) {
        "kasia-indexer.exe"
    } else {
        "kasia-indexer"
    };
    let dest_path = target_dir.join(dest_filename);

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

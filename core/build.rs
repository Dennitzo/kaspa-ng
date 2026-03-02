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

    sync_external_repo_if_needed("rusty-kaspa", "https://github.com/kaspanet/rusty-kaspa.git")?;
    export_rusty_kaspa_workspace_version()?;
    prepare_self_hosted_python_if_needed()?;
    ensure_postgres_runtime_ready()?;

    let external_enabled = external_builds_enabled();
    if external_enabled {
        ensure_node_and_npm_ready()?;
    }

    if external_enabled {
        sync_external_repo_if_needed("K", "https://github.com/thesheepcat/K.git")?;
        sync_external_repo_if_needed("K-indexer", "https://github.com/thesheepcat/K-indexer.git")?;
        sync_external_repo_if_needed("Kasia", "https://github.com/K-Kluster/Kasia.git")?;
        sync_external_repo_if_needed("kasvault", "https://github.com/coderofstuff/kasvault.git")?;
    }
    sync_external_repo_if_needed(
        "simply-kaspa-indexer",
        "https://github.com/supertypo/simply-kaspa-indexer.git",
    )?;
    sync_external_repo_if_needed(
        "kasia-indexer",
        "https://github.com/K-Kluster/kasia-indexer.git",
    )?;
    if external_enabled {
        build_explorer_if_needed()?;
        build_k_social_if_needed()?;
        build_kasia_if_needed()?;
        build_kasvault_if_needed()?;
    } else {
        println!("cargo:warning=Skipping external web builds (KASPA_NG_SKIP_EXTERNAL_BUILDS=1)");
    }
    build_simply_kaspa_indexer_if_needed()?;
    if external_enabled {
        build_k_indexer_if_needed()?;
        build_kasia_indexer_if_needed()?;
    }
    build_stratum_bridge_if_needed()?;
    Ok(())
}

const MIN_NODE_MAJOR: u32 = 22;

fn ensure_node_and_npm_ready() -> Result<(), Box<dyn Error>> {
    let npm_cmd = resolve_npm_command();
    if node_and_npm_ready(MIN_NODE_MAJOR, npm_cmd.as_deref()) {
        if let Some(npm) = npm_cmd {
            println!("cargo:warning=Using npm command: {npm}");
        }
        return Ok(());
    }

    let node_info = detected_node_info()
        .map(|(cmd, version, major)| format!("{cmd}={version} (major {major})"))
        .unwrap_or_else(|| "not found".to_string());
    let npm_info = npm_cmd
        .as_deref()
        .and_then(|cmd| {
            command_output_line(cmd, &["--version"]).map(|version| format!("{cmd}={version}"))
        })
        .unwrap_or_else(|| "not found (tried NPM env, PATH and node-adjacent npm)".to_string());

    Err(format!(
        "Node.js/npm prerequisite check failed. Ensure node >= {MIN_NODE_MAJOR} and npm are installed and reachable. build.rs auto-detects npm via NPM env, PATH and common node-adjacent paths. Detected: node[{node_info}], npm[{npm_info}]"
    )
    .into())
}

fn node_and_npm_ready(min_major: u32, npm_cmd: Option<&str>) -> bool {
    let npm_ok = npm_cmd
        .map(|cmd| command_succeeds(cmd, &["--version"]))
        .unwrap_or(false);
    let node_ok = detected_node_info()
        .map(|(_, _, major)| major >= min_major)
        .unwrap_or(false);
    node_ok && npm_ok
}

fn resolve_npm_command() -> Option<String> {
    if let Ok(npm_cmd) = std::env::var("NPM") {
        let trimmed = npm_cmd.trim();
        if !trimmed.is_empty() && command_succeeds(trimmed, &["--version"]) {
            return Some(trimmed.to_string());
        }
    }

    let mut candidates: Vec<String> = vec!["npm".to_string()];
    #[cfg(windows)]
    {
        candidates.push("npm.cmd".to_string());
        candidates.push("npm.exe".to_string());
    }

    if let Some((node_cmd, _, _)) = detected_node_info() {
        let node_path = if PathBuf::from(&node_cmd).components().count() > 1 {
            Some(PathBuf::from(&node_cmd))
        } else {
            locate_executable_in_path(&node_cmd)
        };

        if let Some(node_path) = node_path
            && let Some(parent) = node_path.parent()
        {
            #[cfg(windows)]
            {
                candidates.push(parent.join("npm.cmd").to_string_lossy().to_string());
                candidates.push(parent.join("npm.exe").to_string_lossy().to_string());
            }
            #[cfg(not(windows))]
            {
                candidates.push(parent.join("npm").to_string_lossy().to_string());
            }
        }
    }

    candidates
        .into_iter()
        .find(|cmd| command_succeeds(cmd, &["--version"]))
}

fn locate_executable_in_path(program: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    let locator = "where";
    #[cfg(not(windows))]
    let locator = "which";

    let output = Command::new(locator).arg(program).output().ok()?;
    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .find(|path| path.exists())
}

fn detected_node_info() -> Option<(String, String, u32)> {
    let mut candidates: Vec<String> = Vec::new();
    if let Ok(node_cmd) = std::env::var("NODE") {
        let trimmed = node_cmd.trim();
        if !trimmed.is_empty() {
            candidates.push(trimmed.to_string());
        }
    }
    candidates.push("node".to_string());
    candidates.push("nodejs".to_string());

    for cmd in candidates {
        if let Some(version) = command_output_line(&cmd, &["--version"])
            && let Some(major) = parse_node_major(version.as_str())
        {
            return Some((cmd, version, major));
        }
    }
    None
}

fn parse_node_major(version: &str) -> Option<u32> {
    let trimmed = version.trim().trim_start_matches('v');
    trimmed.split('.').next()?.parse::<u32>().ok()
}

fn command_output_line(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if line.is_empty() { None } else { Some(line) }
}

fn command_succeeds(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn command_path_succeeds(program: &Path, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn is_truthy_env(var: &str) -> bool {
    std::env::var(var)
        .map(|value| {
            value == "1" || value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("yes")
        })
        .unwrap_or(false)
}

fn ensure_postgres_runtime_ready() -> Result<(), Box<dyn Error>> {
    if is_truthy_env("KASPA_NG_SKIP_POSTGRES_RUNTIME_SETUP") {
        println!(
            "cargo:warning=Skipping postgres runtime staging (KASPA_NG_SKIP_POSTGRES_RUNTIME_SETUP=1)"
        );
        return Ok(());
    }

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let repo_root = manifest_dir
        .parent()
        .ok_or("failed to resolve repo root for postgres runtime staging")?
        .to_path_buf();
    let target_root = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_root.join("target"));
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let out_dir = target_root.join(profile).join("postgres");

    let staged_postgres = out_dir.join("bin").join(if cfg!(windows) {
        "postgres.exe"
    } else {
        "postgres"
    });

    if command_path_succeeds(&staged_postgres, &["--version"]) {
        return Ok(());
    }

    #[cfg(windows)]
    {
        let script = repo_root.join("scripts").join("stage-postgres-runtime.ps1");
        println!("cargo:rerun-if-changed={}", script.display());
        println!(
            "cargo:warning=Staging PostgreSQL runtime to {}",
            out_dir.display()
        );

        let out_dir_arg = out_dir.to_string_lossy().to_string();
        let repo_root_arg = repo_root.to_string_lossy().to_string();
        let script_arg = script.to_string_lossy().to_string();

        let mut success = false;
        for shell in ["pwsh", "powershell"] {
            let status = Command::new(shell)
                .args([
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    script_arg.as_str(),
                    "-RepoRoot",
                    repo_root_arg.as_str(),
                    "-OutDir",
                    out_dir_arg.as_str(),
                ])
                .status();
            if matches!(status, Ok(s) if s.success()) {
                success = true;
                break;
            }
        }

        if !success {
            return Err(format!(
                "failed to stage PostgreSQL runtime via scripts/stage-postgres-runtime.ps1 (target: {})",
                out_dir.display()
            )
            .into());
        }
    }

    #[cfg(not(windows))]
    {
        let script = repo_root.join("scripts").join("stage-postgres-runtime.sh");
        println!("cargo:rerun-if-changed={}", script.display());
        println!(
            "cargo:warning=Staging PostgreSQL runtime to {}",
            out_dir.display()
        );

        let status = Command::new("bash")
            .arg(script)
            .arg(out_dir.as_os_str())
            .status();
        if !matches!(status, Ok(s) if s.success()) {
            return Err(format!(
                "failed to stage PostgreSQL runtime via scripts/stage-postgres-runtime.sh (target: {})",
                out_dir.display()
            )
            .into());
        }
    }

    if !command_path_succeeds(&staged_postgres, &["--version"]) {
        return Err(format!(
            "staged PostgreSQL runtime is not usable at {}",
            staged_postgres.display()
        )
        .into());
    }

    Ok(())
}

fn prepare_self_hosted_python_if_needed() -> Result<(), Box<dyn Error>> {
    if std::env::var("KASPA_NG_SKIP_PYTHON_SETUP")
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

    let launcher = match find_python_launcher() {
        Some(launcher) => launcher,
        None => {
            println!(
                "cargo:warning=Python launcher not found during build; skipping self-hosted python setup"
            );
            return Ok(());
        }
    };

    let rest_root = repo_root.join("kaspa-rest-server");
    prepare_python_server_env(
        &launcher,
        &rest_root,
        &rest_root.join("pyproject.toml"),
        &[
            "uvicorn",
            "fastapi_utils",
            "typing_inspect",
            "sqlalchemy",
            "gunicorn",
            "asyncpg",
            "grpc",
        ],
        &[
            "gunicorn",
            "uvicorn",
            "fastapi",
            "fastapi-utils",
            "typing-inspect",
            "sqlalchemy",
            "grpcio",
            "grpcio-tools",
            "requests",
            "websockets",
            "asyncpg",
            "cachetools",
            "aiohttp",
            "aiocache",
            "psycopg2-binary",
            "waitress",
            "starlette",
            "greenlet",
            "kaspa-script-address",
            "kaspa",
        ],
    )?;

    let socket_root = repo_root.join("kaspa-socket-server");
    prepare_python_server_env(
        &launcher,
        &socket_root,
        &socket_root.join("Pipfile"),
        &[
            "uvicorn",
            "fastapi_utils",
            "typing_inspect",
            "sqlalchemy",
            "gunicorn",
            "socketio",
            "asyncpg",
            "grpc",
        ],
        &[
            "gunicorn",
            "uvicorn",
            "fastapi",
            "fastapi-utils",
            "typing-inspect",
            "sqlalchemy",
            "python-socketio",
            "grpcio",
            "grpcio-tools",
            "requests",
            "websockets",
            "asyncpg",
            "cachetools",
            "psycopg2-binary",
        ],
    )?;

    Ok(())
}

fn find_python_launcher() -> Option<String> {
    #[cfg(windows)]
    let candidates = [
        "py -3.12",
        "py -3.11",
        "py -3.10",
        "py -3",
        "python3.12",
        "python3.11",
        "python",
        "python3",
    ];
    #[cfg(not(windows))]
    let candidates = [
        "python3.12",
        "python3.11",
        "python3.10",
        "python3",
        "python",
    ];

    for candidate in candidates {
        let mut parts = candidate.split_whitespace();
        let program = match parts.next() {
            Some(p) => p,
            None => continue,
        };
        let args: Vec<&str> = parts.collect();

        let mut cmd = Command::new(program);
        for arg in &args {
            cmd.arg(arg);
        }
        let ok = cmd
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if ok {
            return Some(candidate.to_string());
        }
    }

    None
}

fn prepare_python_server_env(
    launcher: &str,
    root: &Path,
    manifest: &Path,
    modules: &[&str],
    pip_packages: &[&str],
) -> Result<(), Box<dyn Error>> {
    if !root.exists() {
        return Ok(());
    }

    println!("cargo:rerun-if-changed={}", root.join("main.py").display());
    println!("cargo:rerun-if-changed={}", manifest.display());

    let venv_dir = root.join(".venv");
    let venv_python = if cfg!(windows) {
        venv_dir.join("Scripts").join("python.exe")
    } else {
        venv_dir.join("bin").join("python3")
    };

    if venv_python.exists() && !python_interpreter_works(&venv_python) {
        println!(
            "cargo:warning=Recreating python virtualenv for {} (existing venv python is not runnable)",
            root.display()
        );
        let _ = std::fs::remove_dir_all(&venv_dir);
    }

    if venv_python.exists()
        && venv_python_is_too_new_for_runtime_deps(&venv_python)
        && launcher_prefers_older_python(launcher)
    {
        println!(
            "cargo:warning=Recreating python virtualenv for {} (existing venv python is too new)",
            root.display()
        );
        let _ = std::fs::remove_dir_all(&venv_dir);
    }

    if !venv_python.exists() {
        eprintln!("build: creating python virtualenv for {}", root.display());
        let mut launcher_parts = launcher.split_whitespace();
        let launcher_program = launcher_parts
            .next()
            .ok_or("invalid python launcher")?
            .to_string();
        let launcher_args: Vec<String> = launcher_parts.map(|s| s.to_string()).collect();
        let mut created = false;
        for attempt in 1..=2 {
            let mut cmd = Command::new(&launcher_program);
            for arg in &launcher_args {
                cmd.arg(arg);
            }
            let status = cmd
                .arg("-m")
                .arg("venv")
                .arg("--clear")
                .arg(&venv_dir)
                .current_dir(root)
                .status()?;
            if status.success() {
                created = true;
                break;
            }
            if attempt == 1 {
                let _ = std::fs::remove_dir_all(&venv_dir);
            }
        }

        if !created {
            println!(
                "cargo:warning=Failed to create python virtualenv for {}; skipping setup",
                root.display()
            );
            return Ok(());
        }
    }

    if python_modules_available_for_python(&venv_python, modules) {
        return Ok(());
    }

    let _ = run_pip_install_with_retries(
        &venv_python,
        root,
        &["--upgrade", "pip", "setuptools", "wheel"],
        2,
    );

    eprintln!(
        "build: installing python runtime packages for {}",
        root.display()
    );
    let mut install_args = vec![
        "--prefer-binary",
        "--disable-pip-version-check",
        "--retries",
        "5",
        "--timeout",
        "60",
    ];
    install_args.extend(pip_packages.iter().copied());

    let install_ok = run_pip_install_with_retries(&venv_python, root, &install_args, 2)?;
    if !install_ok {
        // Retry in smaller chunks to mitigate transient index/network failures.
        let mut chunk_ok = true;
        for chunk in pip_packages.chunks(6) {
            let mut chunk_args = vec![
                "--prefer-binary",
                "--disable-pip-version-check",
                "--retries",
                "5",
                "--timeout",
                "60",
            ];
            chunk_args.extend(chunk.iter().copied());
            if !run_pip_install_with_retries(&venv_python, root, &chunk_args, 2)? {
                chunk_ok = false;
            }
        }
        if chunk_ok && python_modules_available_for_python(&venv_python, modules) {
            return Ok(());
        }
        println!(
            "cargo:warning=Python dependency install failed for {}; runtime may be unavailable",
            root.display()
        );
    } else if !python_modules_available_for_python(&venv_python, modules) {
        println!(
            "cargo:warning=Python dependency verification failed for {}; runtime may be unavailable",
            root.display()
        );
    }

    Ok(())
}

fn run_pip_install_with_retries(
    python: &Path,
    root: &Path,
    pip_args: &[&str],
    attempts: usize,
) -> Result<bool, Box<dyn Error>> {
    let max_attempts = attempts.max(1);
    for attempt in 1..=max_attempts {
        let mut cmd = Command::new(python);
        cmd.arg("-m").arg("pip").arg("install");
        for arg in pip_args {
            cmd.arg(arg);
        }
        let ok = cmd
            .current_dir(root)
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if ok {
            return Ok(true);
        }
        if attempt < max_attempts {
            println!(
                "cargo:warning=Python pip install attempt {attempt}/{max_attempts} failed for {}; retrying",
                root.display()
            );
        }
    }
    Ok(false)
}

fn python_modules_available_for_python(python: &Path, modules: &[&str]) -> bool {
    modules
        .iter()
        .all(|module| python_module_available_for_python(python, module))
}

fn python_module_available_for_python(python: &Path, module: &str) -> bool {
    Command::new(python)
        .arg("-c")
        .arg(format!("import {module}"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn python_interpreter_works(python: &Path) -> bool {
    Command::new(python)
        .arg("-c")
        .arg("import sys; print(sys.version_info[0])")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn launcher_prefers_older_python(launcher: &str) -> bool {
    launcher.contains("3.12") || launcher.contains("3.11") || launcher.contains("3.10")
}

fn venv_python_is_too_new_for_runtime_deps(venv_python: &Path) -> bool {
    let output = Command::new(venv_python)
        .arg("-c")
        .arg("import sys; print(f'{sys.version_info.major}.{sys.version_info.minor}')")
        .output();
    let Ok(output) = output else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let mut parts = version.split('.');
    let major = parts
        .next()
        .and_then(|p| p.parse::<u32>().ok())
        .unwrap_or(0);
    let minor = parts
        .next()
        .and_then(|p| p.parse::<u32>().ok())
        .unwrap_or(0);
    major > 3 || (major == 3 && minor >= 14)
}

fn external_builds_enabled() -> bool {
    let target = std::env::var("TARGET").unwrap_or_default();
    let forced = std::env::var("KASPA_NG_FORCE_EXTERNAL_BUILDS")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if target.contains("wasm32") && !forced {
        return false;
    }

    !std::env::var("KASPA_NG_SKIP_EXTERNAL_BUILDS")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn is_clippy_build() -> bool {
    for key in ["RUSTC_WORKSPACE_WRAPPER", "RUSTC_WRAPPER"] {
        if let Ok(value) = std::env::var(key) {
            let lower = value.to_ascii_lowercase();
            if lower.contains("clippy-driver") || lower.contains("clippy") {
                return true;
            }
        }
    }
    std::env::var("CLIPPY_ARGS").is_ok() || std::env::var("CARGO_CFG_CLIPPY").is_ok()
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
    ensure_external_repo_not_tracked(&repo_root, name);
    let target = repo_root.join(name);

    if !target.exists() {
        println!("cargo:warning=Cloning {name}...");
        clone_external_repo(&repo_root, name, url)?;
        return Ok(());
    }

    if !target.join(".git").exists() {
        println!("cargo:warning={name} has no .git; replacing with a fresh clone");
        let _ = std::fs::remove_dir_all(&target);
        clone_external_repo(&repo_root, name, url)?;
        return Ok(());
    }

    let status = Command::new("git")
        .current_dir(&target)
        .args(["pull", "--ff-only"])
        .status();
    if status.map(|s| !s.success()).unwrap_or(true) {
        return Err(format!("Failed to update {name} via git pull --ff-only").into());
    }

    Ok(())
}

fn ensure_external_repo_not_tracked(repo_root: &Path, name: &str) {
    // Keep external repos out of the index when .gitignore rules are added/changed.
    // This is best-effort and intentionally non-fatal.
    let _ = Command::new("git")
        .current_dir(repo_root)
        .args(["rm", "-r", "--cached", "--ignore-unmatch", name])
        .status();
}

fn clone_external_repo(repo_root: &Path, name: &str, url: &str) -> Result<(), Box<dyn Error>> {
    let status = Command::new("git")
        .current_dir(repo_root)
        .args(["clone", "--depth", "1", url, name])
        .status();
    if status.map(|s| s.success()).unwrap_or(false) {
        Ok(())
    } else {
        Err(format!("Failed to clone external repo {name} from {url}").into())
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
    let npm = resolve_npm_command()
        .ok_or("npm command not found (checked NPM env, PATH and common node-adjacent paths)")?;
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
            return Err("kaspa-explorer-ng npm install failed".into());
        }
    }

    let status = npm_cmd(&["run", "build"]).status();

    if status.map(|s| !s.success()).unwrap_or(true) {
        return Err("kaspa-explorer-ng build failed".into());
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

    // Keep upstream repos unmodified during build; no local source patching.

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
    let npm = resolve_npm_command()
        .ok_or("npm command not found (checked NPM env, PATH and common node-adjacent paths)")?;
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
            return Err("K npm install failed".into());
        }
    }

    let status = Command::new(&npm)
        .current_dir(&k_root)
        .args(["run", "build"])
        .status();
    if status.map(|s| !s.success()).unwrap_or(true) {
        return Err("K build failed".into());
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
    let cipher_wasm_dir = kasia_root.join("cipher-wasm");
    let cipher_wasm_package = cipher_wasm_dir.join("package.json");
    let biometry_vendor_dir = kasia_root.join("vendors").join("tauri-plugin-biometry");
    let biometry_vendor_package = biometry_vendor_dir.join("package.json");
    let build_index = kasia_root.join("dist").join("index.html");

    println!("cargo:rerun-if-changed={}", package_json.display());
    println!("cargo:rerun-if-changed={}", lockfile.display());
    println!("cargo:rerun-if-changed={}", src_dir.display());
    println!("cargo:rerun-if-changed={}", public_dir.display());
    println!("cargo:rerun-if-changed={}", scripts_dir.display());
    println!("cargo:rerun-if-changed={}", cipher_dir.display());
    println!("cargo:rerun-if-changed={}", cipher_wasm_dir.display());
    println!("cargo:rerun-if-changed={}", wasm_dir.display());
    println!("cargo:rerun-if-env-changed=KASIA_WASM_SDK_URL");
    println!("cargo:rerun-if-env-changed=KASIA_WASM_AUTO_FETCH");
    println!(
        "cargo:rerun-if-changed={}",
        kasia_root.join("vite.config.ts").display()
    );

    // Keep upstream repos unmodified during build; no local patching/stubbing in external repos.

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

    eprintln!("Building Kasia (static)...");
    ensure_kasia_fallback_packages(
        &cipher_wasm_dir,
        &cipher_wasm_package,
        &biometry_vendor_dir,
        &biometry_vendor_package,
    )?;
    let npm = resolve_npm_command()
        .ok_or("npm command not found (checked NPM env, PATH and common node-adjacent paths)")?;
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

    if lockfile.exists()
        && let Some(details) = detect_kasia_npm_ci_mismatch(&npm_cmd)
    {
        println!(
            "cargo:warning=Kasia npm ci lockfile mismatch detected; proceeding with npm install fallback. {details}"
        );
    }

    // `npm ci` is strict and fails when lockfiles miss optional platform-specific packages.
    // On CI (especially macOS arm64), this can break otherwise valid Kasia builds.
    // Use `npm install` with optional deps for a resilient cross-platform install.
    let mut ok = npm_cmd(&["install", "--no-audit", "--no-fund"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !ok {
        println!("cargo:warning=Kasia npm install failed; retrying with --include=optional");
        ok = npm_cmd(&["install", "--no-audit", "--no-fund", "--include=optional"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    }

    if !ok {
        return Err("Kasia npm install failed".into());
    }

    if !biometry_vendor_package.exists() {
        println!(
            "cargo:warning=Kasia optional dependency missing: {} (continuing without hard failure)",
            biometry_vendor_package.display()
        );
    }

    if !wasm_package_json.exists() {
        ensure_kasia_wasm_package(&repo_root, &kasia_root, &wasm_dir, &wasm_package_json)?;
    }

    if !wasm_package_json.exists() {
        return Err(format!(
            "Kasia wasm package missing at {}; cannot build Kasia. Set KASIA_WASM_SDK_URL to a valid sdk zip or provide Kasia/wasm/package.json",
            wasm_package_json.display()
        )
        .into());
    }

    // npm optional native packages are frequently missing on CI/local due npm optional-dep bugs.
    // Proactively repair before build to avoid false-negative Kasia build failures.
    let _ = ensure_kasia_native_optional_deps(&npm_cmd);
    let _ = prune_kasia_nested_swc_native_binding(&kasia_root);

    let wasm_built = build_kasia_cipher_wasm_no_opt(&kasia_root);
    if !wasm_built {
        let wasm_status = npm_cmd(&["run", "wasm:build"]).status();
        if wasm_status.map(|s| !s.success()).unwrap_or(true) {
            println!(
                "cargo:warning=Kasia wasm:build failed; restoring fallback cipher package and continuing with production build"
            );
            ensure_kasia_fallback_packages(
                &cipher_wasm_dir,
                &cipher_wasm_package,
                &biometry_vendor_dir,
                &biometry_vendor_package,
            )?;
        }
    }

    let mut status_ok = npm_cmd(&["run", "build:production"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !status_ok {
        let repaired = ensure_kasia_native_optional_deps(&npm_cmd);
        if repaired {
            println!(
                "cargo:warning=Kasia build:production failed; retried after repairing native npm optional deps"
            );
            let _ = prune_kasia_nested_swc_native_binding(&kasia_root);
            status_ok = npm_cmd(&["run", "build:production"])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
        }
    }

    if !status_ok {
        println!("cargo:warning=Kasia build:production failed; trying vite build fallback");
        let mut fallback_ok = npm_cmd(&["exec", "--", "vite", "build"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !fallback_ok {
            let repaired = ensure_kasia_native_optional_deps(&npm_cmd);
            if repaired {
                println!(
                    "cargo:warning=Kasia vite fallback failed; retrying after repairing native npm optional deps"
                );
                let _ = prune_kasia_nested_swc_native_binding(&kasia_root);
                fallback_ok = npm_cmd(&["exec", "--", "vite", "build"])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
            }
        }
        if !fallback_ok {
            let deep_repaired = reset_kasia_native_bindings(&kasia_root, &npm_cmd);
            if deep_repaired {
                println!(
                    "cargo:warning=Kasia build failed after optional-deps repair; retrying after deep native reset"
                );
                let mut recovered = npm_cmd(&["run", "build:production"])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
                if !recovered {
                    recovered = npm_cmd(&["exec", "--", "vite", "build"])
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false);
                }
                if !recovered {
                    return Err("Kasia build failed".into());
                }
            } else {
                return Err("Kasia build failed".into());
            }
        }
    }

    sync_kasia_build(&kasia_root, &repo_root)?;
    Ok(())
}

fn build_kasia_cipher_wasm_no_opt(kasia_root: &Path) -> bool {
    let cipher_dir = kasia_root.join("cipher");
    if !cipher_dir.exists() {
        return false;
    }

    let status = Command::new("wasm-pack")
        .current_dir(&cipher_dir)
        .args([
            "build",
            "--target",
            "web",
            "--release",
            "--no-opt",
            "-d",
            "../cipher-wasm",
        ])
        .status();

    match status {
        Ok(s) if s.success() => true,
        Ok(_) => {
            println!(
                "cargo:warning=Kasia wasm-pack --no-opt build failed; falling back to npm wasm:build"
            );
            false
        }
        Err(_) => {
            println!(
                "cargo:warning=wasm-pack not found for Kasia wasm build; falling back to npm wasm:build"
            );
            false
        }
    }
}

fn kasia_native_optional_packages() -> Vec<&'static str> {
    let mut packages = Vec::new();

    // Rollup native package
    let rollup_pkg = if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            Some("@rollup/rollup-darwin-arm64")
        } else if cfg!(target_arch = "x86_64") {
            Some("@rollup/rollup-darwin-x64")
        } else {
            None
        }
    } else if cfg!(target_os = "linux") {
        if cfg!(target_arch = "x86_64") {
            Some("@rollup/rollup-linux-x64-gnu")
        } else if cfg!(target_arch = "aarch64") {
            Some("@rollup/rollup-linux-arm64-gnu")
        } else {
            None
        }
    } else if cfg!(target_os = "windows") {
        if cfg!(target_arch = "x86_64") {
            Some("@rollup/rollup-win32-x64-msvc")
        } else if cfg!(target_arch = "aarch64") {
            Some("@rollup/rollup-win32-arm64-msvc")
        } else {
            None
        }
    } else {
        None
    };

    if let Some(pkg) = rollup_pkg {
        packages.push(pkg);
    }

    // Lightning CSS native package
    let lightningcss_pkg = if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            Some("lightningcss-darwin-arm64")
        } else if cfg!(target_arch = "x86_64") {
            Some("lightningcss-darwin-x64")
        } else {
            None
        }
    } else if cfg!(target_os = "linux") {
        if cfg!(target_arch = "x86_64") {
            Some("lightningcss-linux-x64-gnu")
        } else if cfg!(target_arch = "aarch64") {
            Some("lightningcss-linux-arm64-gnu")
        } else {
            None
        }
    } else if cfg!(target_os = "windows") {
        if cfg!(target_arch = "x86_64") {
            Some("lightningcss-win32-x64-msvc")
        } else if cfg!(target_arch = "aarch64") {
            Some("lightningcss-win32-arm64-msvc")
        } else {
            None
        }
    } else {
        None
    };

    if let Some(pkg) = lightningcss_pkg {
        packages.push(pkg);
    }

    // Tailwind Oxide native package
    let tailwind_oxide_pkg = if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            Some("@tailwindcss/oxide-darwin-arm64")
        } else if cfg!(target_arch = "x86_64") {
            Some("@tailwindcss/oxide-darwin-x64")
        } else {
            None
        }
    } else if cfg!(target_os = "linux") {
        if cfg!(target_arch = "x86_64") {
            Some("@tailwindcss/oxide-linux-x64-gnu")
        } else if cfg!(target_arch = "aarch64") {
            Some("@tailwindcss/oxide-linux-arm64-gnu")
        } else {
            None
        }
    } else if cfg!(target_os = "windows") {
        if cfg!(target_arch = "x86_64") {
            Some("@tailwindcss/oxide-win32-x64-msvc")
        } else if cfg!(target_arch = "aarch64") {
            Some("@tailwindcss/oxide-win32-arm64-msvc")
        } else {
            None
        }
    } else {
        None
    };

    if let Some(pkg) = tailwind_oxide_pkg {
        packages.push(pkg);
    }

    // Esbuild native package
    let esbuild_pkg = if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            Some("@esbuild/darwin-arm64")
        } else if cfg!(target_arch = "x86_64") {
            Some("@esbuild/darwin-x64")
        } else {
            None
        }
    } else if cfg!(target_os = "linux") {
        if cfg!(target_arch = "x86_64") {
            Some("@esbuild/linux-x64")
        } else if cfg!(target_arch = "aarch64") {
            Some("@esbuild/linux-arm64")
        } else {
            None
        }
    } else if cfg!(target_os = "windows") {
        if cfg!(target_arch = "x86_64") {
            Some("@esbuild/win32-x64")
        } else if cfg!(target_arch = "aarch64") {
            Some("@esbuild/win32-arm64")
        } else {
            None
        }
    } else {
        None
    };

    if let Some(pkg) = esbuild_pkg {
        packages.push(pkg);
    }

    // SWC native package used by @swc/core (required by Vite config in Kasia).
    let swc_pkg = if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            Some("@swc/core-darwin-arm64")
        } else if cfg!(target_arch = "x86_64") {
            Some("@swc/core-darwin-x64")
        } else {
            None
        }
    } else if cfg!(target_os = "linux") {
        if cfg!(target_arch = "x86_64") {
            Some("@swc/core-linux-x64-gnu")
        } else if cfg!(target_arch = "aarch64") {
            Some("@swc/core-linux-arm64-gnu")
        } else {
            None
        }
    } else if cfg!(target_os = "windows") {
        if cfg!(target_arch = "x86_64") {
            Some("@swc/core-win32-x64-msvc")
        } else if cfg!(target_arch = "aarch64") {
            Some("@swc/core-win32-arm64-msvc")
        } else {
            None
        }
    } else {
        None
    };

    if let Some(pkg) = swc_pkg {
        packages.push(pkg);
    }

    packages
}

fn ensure_kasia_native_optional_deps(npm_cmd: &dyn Fn(&[&str]) -> std::process::Command) -> bool {
    let packages = kasia_native_optional_packages();
    if packages.is_empty() {
        return false;
    }

    let mut args = vec!["install", "--no-save", "--no-audit", "--no-fund"];
    args.extend(packages.iter().copied());

    let package_list = packages.join(", ");
    eprintln!("build: Kasia self-heal installing native npm optional deps ({package_list})");

    npm_cmd(&args)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn detect_kasia_npm_ci_mismatch(
    npm_cmd: &dyn Fn(&[&str]) -> std::process::Command,
) -> Option<String> {
    let output = npm_cmd(&["ci", "--dry-run", "--no-audit", "--no-fund"])
        .output()
        .ok()?;
    if output.status.success() {
        return None;
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let lock_mismatch = stderr
        .contains("can only install packages when your package.json and package-lock.json")
        || stderr.contains("are in sync")
        || stderr.contains("Missing:");
    if !lock_mismatch {
        return None;
    }

    let mut missing_examples = Vec::new();
    for line in stderr.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("npm error Missing:") {
            missing_examples.push(trimmed.trim_start_matches("npm error ").to_string());
            if missing_examples.len() >= 3 {
                break;
            }
        }
    }

    if missing_examples.is_empty() {
        Some(
            "package-lock.json is not fully in sync with package.json/optional platform packages"
                .to_string(),
        )
    } else {
        Some(format!("examples: {}", missing_examples.join(" | ")))
    }
}

fn reset_kasia_native_bindings(
    kasia_root: &Path,
    npm_cmd: &dyn Fn(&[&str]) -> std::process::Command,
) -> bool {
    let native_dirs = [
        kasia_root.join("node_modules").join("@swc"),
        kasia_root.join("node_modules").join("@esbuild"),
        kasia_root.join("node_modules").join("@rollup"),
        kasia_root.join("node_modules").join("lightningcss"),
        kasia_root
            .join("node_modules")
            .join("@tailwindcss")
            .join("oxide"),
    ];
    for dir in native_dirs {
        if dir.exists() {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    let reinstall_ok = npm_cmd(&["install", "--no-audit", "--no-fund", "--include=optional"])
        .status()
        .map(|status| status.success())
        .unwrap_or(false);

    reinstall_ok && ensure_kasia_native_optional_deps(npm_cmd)
}

fn prune_kasia_nested_swc_native_binding(kasia_root: &Path) -> bool {
    let nested_parent = kasia_root
        .join("node_modules")
        .join("@swc")
        .join("core")
        .join("node_modules");
    if !nested_parent.exists() {
        return false;
    }

    let entries = match std::fs::read_dir(&nested_parent) {
        Ok(entries) => entries,
        Err(_) => return false,
    };

    let mut removed_any = false;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if (name.starts_with("@swc") || name.starts_with("core-"))
            && std::fs::remove_dir_all(&path).is_ok()
        {
            removed_any = true;
        }
    }

    removed_any
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
    let npm = resolve_npm_command()
        .ok_or("npm command not found (checked NPM env, PATH and common node-adjacent paths)")?;
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
            return Err("KasVault npm install failed".into());
        }
    }

    let status = Command::new(&npm)
        .current_dir(&kasvault_root)
        .env("DISABLE_ESLINT_PLUGIN", "true")
        .args(["run", "build"])
        .status();
    if status.map(|s| !s.success()).unwrap_or(true) {
        return Err("KasVault build failed".into());
    }

    sync_kasvault_build(&kasvault_root, &repo_root)?;
    Ok(())
}

#[allow(dead_code)]
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

fn ensure_kasia_wasm_package(
    repo_root: &Path,
    _kasia_root: &Path,
    wasm_dir: &Path,
    wasm_package_json: &Path,
) -> Result<(), Box<dyn Error>> {
    if wasm_package_json.exists() {
        if kasia_wasm_package_is_compatible(wasm_dir) {
            return Ok(());
        }
        println!(
            "cargo:warning=Existing Kasia wasm package is incompatible (missing required exports); refreshing"
        );
        let _ = std::fs::remove_dir_all(wasm_dir);
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
        if !kasia_wasm_package_is_compatible(&source_wasm_dir) {
            println!(
                "cargo:warning=Kasia wasm package from {url} is incompatible (missing required exports)"
            );
            continue;
        }

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
    let unzip_ok = Command::new("unzip")
        .args(["-q"])
        .arg(archive)
        .args(["-d"])
        .arg(destination)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if unzip_ok {
        return true;
    }

    let tar_ok = Command::new("tar")
        .args(["-xf"])
        .arg(archive)
        .args(["-C"])
        .arg(destination)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if tar_ok {
        return true;
    }

    #[cfg(windows)]
    {
        let script = format!(
            "Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
            archive.display(),
            destination.display()
        );
        let ps_ok = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ps_ok {
            return true;
        }
    }

    false
}

fn ensure_kasia_fallback_packages(
    cipher_wasm_dir: &Path,
    cipher_wasm_package: &Path,
    biometry_vendor_dir: &Path,
    biometry_vendor_package: &Path,
) -> Result<(), Box<dyn Error>> {
    if !cipher_wasm_package_is_usable(cipher_wasm_dir, cipher_wasm_package) {
        std::fs::create_dir_all(cipher_wasm_dir)?;
        std::fs::write(
            cipher_wasm_package,
            r#"{
  "name": "cipher",
  "version": "0.0.0-fallback",
  "type": "module",
  "main": "index.js",
  "types": "index.d.ts"
}
"#,
        )?;
        std::fs::write(
            cipher_wasm_dir.join("index.js"),
            r#"class EncryptedMessage {
  constructor(hex = "") {
    this.hex = String(hex || "");
  }
  to_hex() {
    return this.hex;
  }
}

class PrivateKey {
  constructor(value = "") {
    this.value = String(value || "");
  }
  toString() {
    return this.value;
  }
}

const encoder = new TextEncoder();
const decoder = new TextDecoder();

function toHex(text) {
  return Array.from(encoder.encode(String(text)))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

function fromHex(hex) {
  const clean = String(hex || "");
  if (clean.length % 2 !== 0) return "";
  const bytes = new Uint8Array(clean.length / 2);
  for (let i = 0; i < clean.length; i += 2) {
    const value = Number.parseInt(clean.slice(i, i + 2), 16);
    if (Number.isNaN(value)) return "";
    bytes[i / 2] = value;
  }
  return decoder.decode(bytes);
}

export function encrypt_message(_address, message) {
  return new EncryptedMessage(toHex(message));
}

export function decrypt_message(encryptedMessage, _privateKey) {
  const hex =
    encryptedMessage && typeof encryptedMessage.to_hex === "function"
      ? encryptedMessage.to_hex()
      : String(encryptedMessage || "");
  return fromHex(hex);
}

export async function defaultInit() {}
export default defaultInit;
export { EncryptedMessage, PrivateKey };
"#,
        )?;
        std::fs::write(
            cipher_wasm_dir.join("index.d.ts"),
            r#"export default function initCipherWasm(): Promise<void>;
export class EncryptedMessage {
  constructor(hex?: string);
  to_hex(): string;
}
export class PrivateKey {
  constructor(value?: string);
  toString(): string;
}
export function encrypt_message(address: string, message: string): EncryptedMessage;
export function decrypt_message(
  encryptedMessage: EncryptedMessage,
  privateKey: PrivateKey
): string;
"#,
        )?;
        println!(
            "cargo:warning=Created fallback cipher package at {}",
            cipher_wasm_dir.display()
        );
    }

    if !biometry_vendor_package.exists() {
        std::fs::create_dir_all(biometry_vendor_dir)?;
        std::fs::write(
            biometry_vendor_package,
            r#"{
  "name": "@tauri-apps/plugin-biometry",
  "version": "0.0.0-fallback",
  "type": "module",
  "main": "index.js",
  "types": "index.d.ts"
}
"#,
        )?;
        std::fs::write(
            biometry_vendor_dir.join("index.js"),
            r#"export async function hasData() {
  return false;
}
export async function setData() {}
export async function getData() {
  return null;
}
export async function checkStatus() {
  return { isAvailable: false };
}
"#,
        )?;
        std::fs::write(
            biometry_vendor_dir.join("index.d.ts"),
            r#"export type BiometryPayload = {
  domain: string;
  name: string;
  data?: string;
  reason?: string;
  cancelTitle?: string;
};

export function hasData(payload: BiometryPayload): Promise<boolean>;
export function setData(payload: BiometryPayload): Promise<void>;
export function getData(
  payload: BiometryPayload
): Promise<{ data: string } | null>;
export function checkStatus(): Promise<{ isAvailable: boolean }>;
"#,
        )?;
        println!(
            "cargo:warning=Created fallback biometry package at {}",
            biometry_vendor_dir.display()
        );
    }

    Ok(())
}

fn cipher_wasm_package_is_usable(cipher_wasm_dir: &Path, cipher_wasm_package: &Path) -> bool {
    if !cipher_wasm_package.exists() {
        return false;
    }

    let pkg_content = std::fs::read_to_string(cipher_wasm_package).ok();
    let has_cipher_name = pkg_content
        .as_deref()
        .map(|content| content.contains("\"name\"") && content.contains("\"cipher\""))
        .unwrap_or(false);
    if !has_cipher_name {
        return false;
    }

    let js_exists =
        cipher_wasm_dir.join("cipher.js").exists() || cipher_wasm_dir.join("index.js").exists();
    let dts_exists =
        cipher_wasm_dir.join("cipher.d.ts").exists() || cipher_wasm_dir.join("index.d.ts").exists();

    js_exists && dts_exists
}

fn find_kasia_wasm_source_dir(root: &Path) -> Option<PathBuf> {
    // Only accept the browser SDK package (`web/kaspa`).
    // NodeJS variants have the same package name but incompatible exports for Kasia.
    let preferred_candidates = [
        root.join("kaspa-wasm32-sdk").join("web").join("kaspa"),
        root.join("web").join("kaspa"),
        root.join("kaspa"),
    ];
    for candidate in preferred_candidates {
        if candidate
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            == Some("web")
            && kasia_wasm_package_is_compatible(&candidate)
        {
            return Some(candidate);
        }
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
            if path.file_name().and_then(|n| n.to_str()) == Some("kaspa") {
                let is_web_dir = path
                    .parent()
                    .and_then(|parent| parent.file_name())
                    .and_then(|name| name.to_str())
                    == Some("web");
                if is_web_dir && kasia_wasm_package_is_compatible(&path) {
                    return Some(path);
                }
            }
            if let Some(found) = walk(&path, depth + 1) {
                return Some(found);
            }
        }
        None
    }

    walk(root, 0)
}

fn kasia_wasm_package_is_compatible(dir: &Path) -> bool {
    let package_json = dir.join("package.json");
    let kaspa_js = dir.join("kaspa.js");
    let kaspa_dts = dir.join("kaspa.d.ts");
    if !(package_json.exists() && kaspa_js.exists() && kaspa_dts.exists()) {
        return false;
    }

    let package_text = match std::fs::read_to_string(&package_json) {
        Ok(text) => text,
        Err(_) => return false,
    };
    if !package_text.contains("\"kaspa-wasm\"") {
        return false;
    }

    let js_text = match std::fs::read_to_string(&kaspa_js) {
        Ok(text) => text,
        Err(_) => return false,
    };

    let required_exports = [
        "export default",
        "export class RpcClient",
        "export const ConnectStrategy",
        "export const Encoding",
        "export class Resolver",
        "export function initConsolePanicHook",
    ];

    required_exports
        .iter()
        .all(|required| js_text.contains(required))
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

    eprintln!("Building kasia-indexer (release)...");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = child_cargo_command(&cargo, &indexer_root)
        .args(["build", "-p", "indexer", "--release"])
        .status()?;

    if !status.success() {
        return Err("failed to build kasia-indexer".into());
    }

    sync_kasia_indexer_binary(&bin_path, &repo_root)?;
    Ok(())
}

#[allow(dead_code)]
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
    let status = child_cargo_command(&cargo, &rusty_kaspa)
        .args(["build", "-p", "kaspa-stratum-bridge", "--release"])
        .status()?;

    if !status.success() {
        return Err("failed to build kaspa-stratum-bridge".into());
    }

    sync_stratum_bridge_binary(&bin_path, &repo_root)?;

    Ok(())
}

fn build_simply_kaspa_indexer_if_needed() -> Result<(), Box<dyn Error>> {
    if is_clippy_build() {
        println!("cargo:warning=Skipping simply-kaspa-indexer build during clippy");
        return Ok(());
    }

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
    let status = child_cargo_command(&cargo, &indexer_root)
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
    let k_indexer_lock = k_indexer_root.join("Cargo.lock");
    let web_src = k_indexer_root.join("K-webserver").join("src");
    let web_toml = k_indexer_root.join("K-webserver").join("Cargo.toml");
    let processor_src = k_indexer_root.join("K-transaction-processor").join("src");
    let processor_toml = k_indexer_root
        .join("K-transaction-processor")
        .join("Cargo.toml");

    println!("cargo:rerun-if-changed={}", k_indexer_toml.display());
    println!("cargo:rerun-if-changed={}", k_indexer_lock.display());
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
        .chain(newest_mtime(&k_indexer_lock))
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

    eprintln!("Building K-indexer components (release)...");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    normalize_k_indexer_wasm_dependency_lock(&cargo, &k_indexer_root)?;
    let mut build_cmd = child_cargo_command(&cargo, &k_indexer_root);
    build_cmd.args([
        "build",
        "-p",
        "K-webserver",
        "-p",
        "K-transaction-processor",
    ]);
    if k_indexer_lock.exists() {
        build_cmd.arg("--locked");
    } else {
        println!("cargo:warning=K-indexer Cargo.lock missing; building without --locked");
    }
    let status = build_cmd.arg("--release").status()?;

    if !status.success() {
        return Err("failed to build K-indexer components".into());
    }

    sync_k_indexer_binaries(&web_bin, &processor_bin, &repo_root)?;
    Ok(())
}

fn normalize_k_indexer_wasm_dependency_lock(
    cargo: &str,
    k_indexer_root: &Path,
) -> Result<(), Box<dyn Error>> {
    if !k_indexer_root.join("Cargo.lock").exists() {
        println!("cargo:warning=K-indexer Cargo.lock missing; generating lockfile");
        let status = child_cargo_command(cargo, k_indexer_root)
            .args(["generate-lockfile"])
            .status()?;
        if !status.success() {
            return Err("failed to generate K-indexer Cargo.lock".into());
        }
    }

    // Keep K-indexer aligned with the Rusty-Kaspa wasm dependency stack used by kaspa-ng.
    // Without this, fresh lockfiles can pull newer wasm-bindgen/js-sys releases that break
    // workflow-node and kaspa-rpc-core under current toolchains.
    let required_versions = [
        ("js-sys", "0.3.91"),
        ("web-sys", "0.3.91"),
        ("wasm-bindgen", "0.2.114"),
        ("wasm-bindgen-futures", "0.4.64"),
        ("wasm-bindgen-macro", "0.2.114"),
        ("wasm-bindgen-macro-support", "0.2.114"),
        ("wasm-bindgen-shared", "0.2.114"),
        ("serde-wasm-bindgen", "0.6.5"),
    ];

    for (package, version) in required_versions {
        let status = child_cargo_command(cargo, k_indexer_root)
            .args(["update", "-p", package, "--precise", version])
            .status()?;
        if !status.success() {
            return Err(
                format!("failed to normalize K-indexer dependency {package} to {version}").into(),
            );
        }
    }

    Ok(())
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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

    copy_file_with_windows_lock_tolerance(bin_path, &dest_path)?;
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

    copy_file_with_windows_lock_tolerance(bin_path, &dest_path)?;
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

        copy_file_with_windows_lock_tolerance(bin_path, &dest_path)?;
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

    copy_file_with_windows_lock_tolerance(bin_path, &dest_path)?;
    Ok(())
}

fn child_cargo_command(cargo: &str, cwd: &Path) -> Command {
    let mut cmd = Command::new(cargo);
    cmd.current_dir(cwd);

    // Prevent nested cargo invocations from inheriting wasm/cross-compilation
    // context from the parent build script environment.
    for key in [
        "CARGO_TARGET_DIR",
        "CARGO_BUILD_TARGET",
        "CARGO_ENCODED_RUSTFLAGS",
        "RUSTFLAGS",
        "RUSTC_WRAPPER",
        "TARGET",
        "HOST",
    ] {
        cmd.env_remove(key);
    }

    // Isolate nested builds from the parent cargo target dir to avoid artifact
    // races when build.rs compiles auxiliary workspaces in CI.
    cmd.env("CARGO_TARGET_DIR", cwd.join("target"));

    cmd
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

fn copy_file_with_windows_lock_tolerance(src: &Path, dst: &Path) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }

    match std::fs::copy(src, dst) {
        Ok(_) => Ok(()),
        Err(err) => {
            #[cfg(windows)]
            {
                if err.raw_os_error() == Some(32) {
                    println!(
                        "cargo:warning=Skipping copy to locked destination {}",
                        dst.display()
                    );
                    return Ok(());
                }
            }
            Err(err.into())
        }
    }
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
            copy_file_with_windows_lock_tolerance(&path, &target)
                .map_err(|err| std::io::Error::other(err.to_string()))?;
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

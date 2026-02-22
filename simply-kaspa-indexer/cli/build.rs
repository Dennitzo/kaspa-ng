use std::fs;
use std::path::PathBuf;
use vergen_git2::{Emitter, Git2Builder};

fn main() {
    let git2 = Git2Builder::default().branch(true).commit_date(true).sha(true).describe(true, true, None).build().unwrap();
    Emitter::default().add_instructions(&git2).unwrap().emit().unwrap();

    let version = std::env::var("KASPA_INDEXER_VERSION")
        .ok()
        .or_else(read_workspace_version)
        .unwrap_or_else(|| "v1.1.0-rc.3".to_string());
    println!("cargo:rustc-env=VERGEN_GIT_DESCRIBE={}", version);
}

fn read_workspace_version() -> Option<String> {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").ok()?);
    let workspace_toml = manifest_dir.join("..").join("..").join("..").join("Cargo.toml");
    let content = fs::read_to_string(workspace_toml).ok()?;
    let mut in_workspace_package = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_workspace_package = trimmed == "[workspace.package]";
            continue;
        }
        if !in_workspace_package {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("version") {
            let rest = rest.trim_start();
            if let Some(value) = rest.strip_prefix('=') {
                let value = value.trim().trim_matches('"');
                if !value.is_empty() {
                    return Some(if value.starts_with('v') {
                        value.to_string()
                    } else {
                        format!("v{}", value)
                    });
                }
            }
        }
    }
    None
}

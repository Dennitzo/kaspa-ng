#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
RETRIES="${KASPA_NG_EXTERNAL_SYNC_RETRIES:-4}"
BACKOFF_BASE_SECS="${KASPA_NG_EXTERNAL_SYNC_BACKOFF_SECS:-2}"
STRICT="${KASPA_NG_EXTERNAL_SYNC_STRICT:-1}"

is_truthy() {
  local value="${1:-}"
  case "${value,,}" in
    1|true|yes|on) return 0 ;;
    *) return 1 ;;
  esac
}

remove_target_dir() {
  local path="$1"
  local attempt
  if [[ ! -e "$path" ]]; then
    return 0
  fi

  chmod -R u+w "$path" 2>/dev/null || true
  for attempt in 1 2 3 4 5; do
    rm -rf "$path" 2>/dev/null || true
    [[ ! -e "$path" ]] && return 0
    sleep 0.2
  done
  return 1
}

clone_repo_with_retry() {
  local dir="$1"
  local url="$2"
  local target="$3"
  local attempt
  local sleep_secs

  for ((attempt = 1; attempt <= RETRIES; attempt++)); do
    remove_target_dir "$target" || true
    if git clone --depth 1 "$url" "$target"; then
      return 0
    fi
    if ((attempt < RETRIES)); then
      sleep_secs=$((BACKOFF_BASE_SECS * attempt))
      echo "Clone failed for $dir (attempt ${attempt}/${RETRIES}); retrying in ${sleep_secs}s"
      sleep "$sleep_secs"
    fi
  done
  return 1
}

pull_repo_with_retry() {
  local dir="$1"
  local target="$2"
  local attempt
  local sleep_secs
  local upstream
  local remote
  local branch

  for ((attempt = 1; attempt <= RETRIES; attempt++)); do
    upstream="$(git -C "$target" rev-parse --abbrev-ref --symbolic-full-name '@{upstream}' 2>/dev/null || true)"
    if [[ -n "$upstream" && "$upstream" == */* ]]; then
      remote="${upstream%%/*}"
      branch="${upstream#*/}"
      if git -C "$target" pull --ff-only "$remote" "$branch"; then
        return 0
      fi
    elif git -C "$target" pull --ff-only; then
      return 0
    fi
    if ((attempt < RETRIES)); then
      sleep_secs=$((BACKOFF_BASE_SECS * attempt))
      echo "Pull failed for $dir (attempt ${attempt}/${RETRIES}); retrying in ${sleep_secs}s"
      sleep "$sleep_secs"
    fi
  done
  return 1
}

sync_external_repo() {
  local dir="$1"
  local url="$2"
  local target="$ROOT_DIR/$dir"
  local current_url

  if [[ -d "$target/.git" ]]; then
    current_url="$(git -C "$target" remote get-url origin 2>/dev/null || true)"
    if [[ -n "$current_url" && "$current_url" != "$url" ]]; then
      echo "External repo remote mismatch for $dir; recloning ($current_url -> $url)"
      if ! clone_repo_with_retry "$dir" "$url" "$target"; then
        echo "Failed to re-clone external repo: $dir" >&2
        return 1
      fi
      return 0
    fi

    echo "Updating external repo $dir via git pull --ff-only"
    if pull_repo_with_retry "$dir" "$target"; then
      return 0
    fi

    if is_truthy "$STRICT"; then
      echo "Failed to update external repo: $dir" >&2
      return 1
    fi

    echo "Warning: failed to update $dir; continuing with existing checkout" >&2
    return 0
  fi

  if [[ -e "$target" ]]; then
    echo "External repo $dir exists without .git; recloning"
    remove_target_dir "$target" || true
  fi

  echo "Cloning external repo $dir"
  if ! clone_repo_with_retry "$dir" "$url" "$target"; then
    echo "Failed to clone external repo: $dir" >&2
    return 1
  fi
}

repos=(
  "rusty-kaspa|https://github.com/kaspanet/rusty-kaspa.git"
  "K|https://github.com/thesheepcat/K.git"
  "K-indexer|https://github.com/thesheepcat/K-indexer.git"
  "simply-kaspa-indexer|https://github.com/supertypo/simply-kaspa-indexer.git"
  "Kasia|https://github.com/K-Kluster/Kasia.git"
  "kasia-indexer|https://github.com/K-Kluster/kasia-indexer.git"
  "kasvault|https://github.com/coderofstuff/kasvault.git"
)

for entry in "${repos[@]}"; do
  IFS='|' read -r dir url <<<"$entry"
  sync_external_repo "$dir" "$url"
done

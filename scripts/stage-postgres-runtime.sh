#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-${ROOT}/target/release/postgres}"
REQUIRED_BINS=(postgres initdb pg_ctl)

mkdir -p "$OUT"
rm -rf "$OUT/bin" "$OUT/lib" "$OUT/share"

copy_tree() {
  local src="$1"
  local dst="$2"
  [ -d "$src" ] || return 1
  mkdir -p "$dst"
  (cd "$src" && tar -cf - .) | (cd "$dst" && tar -xf -)
  return 0
}

has_required_bins() {
  local root="$1"
  local bin
  for bin in "${REQUIRED_BINS[@]}"; do
    [ -x "$root/bin/$bin" ] || return 1
  done
  return 0
}

is_usable_root() {
  local root="$1"
  has_required_bins "$root" || return 1
  "$root/bin/postgres" --version >/dev/null 2>&1 || return 1
  "$root/bin/initdb" --version >/dev/null 2>&1 || return 1
  "$root/bin/pg_ctl" --version >/dev/null 2>&1 || return 1
  return 0
}

validate_staged_output() {
  local root="$1"
  local bin
  for bin in "${REQUIRED_BINS[@]}"; do
    [ -x "$root/bin/$bin" ] || return 1
  done
  "$root/bin/postgres" --version >/dev/null 2>&1 || return 1
  "$root/bin/initdb" --version >/dev/null 2>&1 || return 1
  "$root/bin/pg_ctl" --version >/dev/null 2>&1 || return 1
  return 0
}

stage_tree() {
  local source_root="$1"
  local share_path="${2:-}"
  local source_label="$3"

  is_usable_root "$source_root" || return 1
  rm -rf "$OUT/bin" "$OUT/lib" "$OUT/share"
  copy_tree "$source_root/bin" "$OUT/bin" || return 1
  copy_tree "$source_root/lib" "$OUT/lib" || return 1
  if [ -n "$share_path" ] && [ -d "$share_path" ]; then
    copy_tree "$share_path" "$OUT/share" || return 1
  elif [ -d "$source_root/share" ]; then
    copy_tree "$source_root/share" "$OUT/share" || return 1
  else
    return 1
  fi
  validate_staged_output "$OUT" || return 1
  echo "Staged PostgreSQL runtime from ${source_label}: ${source_root} -> $OUT"
  return 0
}

try_stage_from_known_locations() {
  local prefix formula

  # macOS/Homebrew layout.
  if command -v brew >/dev/null 2>&1; then
    for formula in postgresql@17 postgresql@16 postgresql@15 postgresql@14 postgresql; do
      prefix="$(brew --prefix "$formula" 2>/dev/null || true)"
      if [ -n "$prefix" ] && stage_tree "$prefix" "$prefix/share" "Homebrew"; then
        return 0
      fi
    done
  fi

  # Linux distro layout (versioned PostgreSQL tree).
  local linux_dirs=()
  for prefix in /usr/lib/postgresql/*; do
    [ -d "$prefix" ] || continue
    linux_dirs+=("$prefix")
  done
  if [ "${#linux_dirs[@]}" -gt 0 ]; then
    IFS=$'\n' linux_dirs=($(printf '%s\n' "${linux_dirs[@]}" | sort -Vr))
    unset IFS
  fi
  for prefix in "${linux_dirs[@]}"; do
    if stage_tree "$prefix" "/usr/share/postgresql/$(basename "$prefix")" "system package"; then
      return 0
    fi
  done

  # Generic PATH-based installation layout.
  if command -v postgres >/dev/null 2>&1; then
    prefix="$(cd "$(dirname "$(command -v postgres)")/.." && pwd)"
    if stage_tree "$prefix" "$prefix/share" "PATH lookup"; then
      return 0
    fi
  fi

  return 1
}

install_postgres_if_possible() {
  if command -v brew >/dev/null 2>&1; then
    brew install postgresql@15 >/dev/null 2>&1 || brew install postgresql >/dev/null 2>&1 || true
    return 0
  fi

  if command -v apt-get >/dev/null 2>&1; then
    if command -v sudo >/dev/null 2>&1; then
      sudo apt-get update -y >/dev/null 2>&1 || true
      sudo apt-get install -y postgresql >/dev/null 2>&1 || true
    else
      apt-get update -y >/dev/null 2>&1 || true
      apt-get install -y postgresql >/dev/null 2>&1 || true
    fi
    return 0
  fi

  return 1
}

if try_stage_from_known_locations; then
  exit 0
fi

install_postgres_if_possible || true

if try_stage_from_known_locations; then
  exit 0
fi

# If no packaged tree can be staged, keep existing output if valid.
if validate_staged_output "$OUT"; then
  echo "PostgreSQL runtime already present at $OUT"
  exit 0
fi

echo "Unable to stage PostgreSQL runtime (required binaries missing or unusable)." >&2
exit 1

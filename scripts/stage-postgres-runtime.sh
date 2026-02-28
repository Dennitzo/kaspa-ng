#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-${ROOT}/target/release/postgres}"

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

# macOS/Homebrew layout
if command -v brew >/dev/null 2>&1; then
  for formula in postgresql@15 postgresql; do
    prefix="$(brew --prefix "$formula" 2>/dev/null || true)"
    if [ -n "$prefix" ] && [ -x "$prefix/bin/postgres" ]; then
      copy_tree "$prefix/bin" "$OUT/bin" || true
      copy_tree "$prefix/lib" "$OUT/lib" || true
      copy_tree "$prefix/share" "$OUT/share" || true
      echo "Staged PostgreSQL runtime from Homebrew: $prefix -> $OUT"
      exit 0
    fi
  done
fi

# Linux distro layout (postgresql-15)
if [ -x "/usr/lib/postgresql/15/bin/postgres" ]; then
  copy_tree "/usr/lib/postgresql/15/bin" "$OUT/bin" || true
  copy_tree "/usr/lib/postgresql/15/lib" "$OUT/lib" || true
  if [ -d "/usr/share/postgresql/15" ]; then
    copy_tree "/usr/share/postgresql/15" "$OUT/share" || true
  elif [ -d "/usr/lib/postgresql/15/share" ]; then
    copy_tree "/usr/lib/postgresql/15/share" "$OUT/share" || true
  fi
  echo "Staged PostgreSQL runtime from /usr/lib/postgresql/15 -> $OUT"
  exit 0
fi

# If no packaged tree can be staged, keep existing output if valid.
if [ -x "$OUT/bin/postgres" ] || [ -x "$OUT/bin/postgres.exe" ]; then
  echo "PostgreSQL runtime already present at $OUT"
  exit 0
fi

echo "Unable to stage PostgreSQL runtime (bin/postgres not found)." >&2
exit 1
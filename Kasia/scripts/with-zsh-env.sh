#!/bin/zsh
set -euo pipefail

export ZDOTDIR="${ZDOTDIR:-$HOME}"

# Add common Node locations
for p in /opt/homebrew/bin /usr/local/bin "$HOME/.volta/bin" "$HOME/.asdf/shims"; do
  [[ -d "$p" ]] && PATH="$p:$PATH"
done

# nvm (if present)
[[ -s "$HOME/.nvm/nvm.sh" ]] && . "$HOME/.nvm/nvm.sh"

# Run without sourcing .zshrc at all
exec /bin/zsh -lc "$*"

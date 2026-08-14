#!/usr/bin/env bash
# Epic 1.1 — the domain crates (quill-store, quill-mail, quill-cal) must have
# no `tauri` dependency so the domain layer stays portable (plan.md stays
# viable as a fallback). Run from the workspace root.
set -euo pipefail

CRATES=(quill-store quill-mail quill-cal)

for crate in "${CRATES[@]}"; do
  if grep -qE '^\s*tauri([[:space:]]|=)' "crates/$crate/Cargo.toml"; then
    echo "FAIL: crates/$crate/Cargo.toml declares a tauri dependency" >&2
    exit 1
  fi
  # Actual code references, not prose mentions of the word "tauri" in doc
  # comments (which are fine).
  if grep -RnE '(use|extern crate)[[:space:]]+tauri\b|(^|[^[:alnum:]_])tauri::' "crates/$crate/src" >/dev/null 2>&1; then
    echo "FAIL: crates/$crate/src has tauri code references" >&2
    exit 1
  fi
  if cargo tree -p "$crate" --no-dev 2>/dev/null | grep -qiE '\btauri\b'; then
    echo "FAIL: crates/$crate resolves tauri in its dependency graph" >&2
    exit 1
  fi
done

echo "OK: no domain crate depends on tauri"

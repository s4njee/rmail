#!/usr/bin/env bash
set -euo pipefail

echo "=== Checking Seam Isolation (plan.md §8 & S7.3) ==="

ERRORS=0

# Rule 1: packages/calendar-ui must NEVER import @tauri-apps/api
echo -n "Checking calendar-ui for forbidden @tauri-apps/api imports... "
if grep -rn --include="*.ts" --include="*.tsx" --exclude="*.test.ts" "from ['\"]@tauri-apps/api" packages/calendar-ui/src/ ; then
    echo "FAIL: Found @tauri-apps/api imports in packages/calendar-ui/src!"
    ERRORS=$((ERRORS + 1))
else
    echo "PASS"
fi

# Rule 2: crates/calendar-core must NEVER depend on tauri or rusqlite
echo -n "Checking crates/calendar-core/Cargo.toml for forbidden dependencies... "
if grep -E "^(tauri|rusqlite|sqlite)\b" crates/calendar-core/Cargo.toml ; then
    echo "FAIL: Found forbidden dependencies in crates/calendar-core/Cargo.toml!"
    ERRORS=$((ERRORS + 1))
else
    echo "PASS"
fi

if [ "$ERRORS" -gt 0 ]; then
    echo "=== Seam isolation check FAILED with $ERRORS error(s) ==="
    exit 1
fi

echo "=== All seam isolation checks PASSED ==="

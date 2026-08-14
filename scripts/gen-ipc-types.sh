#!/usr/bin/env bash
# Regenerate the TypeScript bindings from the Rust IPC contract (Epic 3.1).
# ts-rs exports during `cargo test`; the output dir is set by TS_RS_EXPORT_DIR
# in .cargo/config.toml. Run from the workspace root.
set -euo pipefail

cargo test -p quill-store
echo "OK: regenerated TS bindings in src/lib/ipc/"

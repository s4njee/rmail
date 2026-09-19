fn main() {
    println!("cargo:rerun-if-changed=icons");
    println!("cargo:rerun-if-changed=tauri.conf.json");
    // Baked into the binary by `oauth_config::build_config`; a new ID must
    // rebuild the crate rather than ship a stale one.
    println!("cargo:rerun-if-env-changed=QUILL_GOOGLE_OAUTH_CLIENT_ID");
    println!("cargo:rerun-if-env-changed=QUILL_GOOGLE_OAUTH_CLIENT_SECRET");
    println!("cargo:rerun-if-env-changed=QUILL_MICROSOFT_OAUTH_CLIENT_ID");
    tauri_build::build();
}

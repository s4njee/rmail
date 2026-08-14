//! Seam isolation test for `calendar-core` (S7.3).
//!
//! Enforces plan.md §8: `calendar-core` must have zero dependencies on `tauri` or `rusqlite`.

#[test]
fn calendar_core_cargo_toml_has_zero_tauri_or_sqlite_dependencies() {
    let cargo_toml_content = include_str!("../Cargo.toml");

    assert!(
        !cargo_toml_content.contains("tauri"),
        "CRITICAL SEAM VIOLATION: calendar-core must NEVER depend on tauri (plan.md §8)"
    );
    assert!(
        !cargo_toml_content.contains("rusqlite") && !cargo_toml_content.contains("sqlite"),
        "CRITICAL SEAM VIOLATION: calendar-core must NEVER depend on rusqlite or sqlite (plan.md §8)"
    );
}

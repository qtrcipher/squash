//! Squash desktop GUI host (Tauri v2, docs/05 §2).
//!
//! The GUI consumes `squash-core` in-process as a crate — no FFI, no IPC
//! serialization layer. Commands are thin async wrappers over the core's job
//! API; those land in Phase 2 alongside the engine. Phase 1 proves linkage
//! and builds the main window shell (docs/03 S1).

/// Sanity command so the command pipeline is exercised before Phase 2.
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {name}! You've been greeted from Squash's Rust host.")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Prove the core is linked: one engine instance, no jobs submitted yet.
    let _engine = squash_core::Engine::new();

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running Squash");
}

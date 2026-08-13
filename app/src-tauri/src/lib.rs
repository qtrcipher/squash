//! Squash desktop GUI host (Tauri v2, docs/05 §2).
//!
//! The GUI consumes `squash-core` in-process as a crate — no FFI, no IPC
//! serialization layer. Commands are thin async wrappers over the core's job
//! API. On launch the persisted queue is restored (docs/06 §2): resubmitted
//! jobs pick up their progress forwarders in `setup`.

mod commands;
mod state;

use state::{spawn_forwarder, AppState, TauriSink};
use std::sync::Arc;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let dirs = squash_core::store::StoreDirs::resolve()
        .expect("could not resolve OS config/data directories");
    let app_state = Arc::new(AppState::new(dirs));

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(Arc::clone(&app_state))
        .setup(move |app| {
            // Restore-on-launch (docs/06 §2): resubmit unfinished jobs, then
            // attach a progress forwarder to each. Events emitted before the
            // frontend subscribes are re-fetched via `list_queue`.
            let restored = app_state.restore_queue();
            for id in restored {
                spawn_forwarder(
                    Arc::clone(&app_state),
                    Arc::new(TauriSink(app.handle().clone())),
                    id,
                );
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::submit_compress,
            commands::submit_extract,
            commands::cancel_job,
            commands::dismiss_job,
            commands::retry_job,
            commands::list_queue,
            commands::get_settings,
            commands::set_settings,
            commands::classify_paths,
            commands::path_exists,
            commands::reveal_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Squash");
}

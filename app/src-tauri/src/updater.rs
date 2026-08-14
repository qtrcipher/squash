//! GUI self-update (docs/03 S6/D3, docs/05 §5): `tauri-plugin-updater` over
//! static JSON manifests on GitHub Releases.
//!
//! Trust posture (docs/02): no silent phone-home. A check only runs when the
//! user clicks "Check for updates" on S6, or on launch when the persisted
//! `update_check_opt_in` consent is on (default off). A check is a single
//! GET of the manifest — the URL carries only the channel; no user data is
//! sent and the downloaded installer is verified against the updater public
//! key baked into the binary.
//!
//! Channels (settings `release_channel`, default `stable`): `stable` reads
//! `latest.json` through GitHub's `releases/latest` alias, which never
//! resolves to a prerelease; `beta` reads `beta.json` from the long-lived
//! `updates` release, which `.github/workflows/release.yml` refreshes on
//! every prerelease tag. The endpoint is selected at runtime here, not in
//! `tauri.conf.json`, because the channel is a user setting.

use serde::Serialize;
use squash_core::store::ReleaseChannel;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

/// Stable manifest: attached to each release by the workflow's finalize job;
/// `releases/latest` always resolves to the newest non-prerelease release.
pub const STABLE_ENDPOINT: &str =
    "https://github.com/qtrcipher/squash/releases/latest/download/latest.json";
/// Beta manifest: `releases/latest` can never point at a prerelease, so
/// prerelease tags refresh `beta.json` on the fixed `updates` release.
pub const BETA_ENDPOINT: &str =
    "https://github.com/qtrcipher/squash/releases/download/updates/beta.json";

/// The manifest URL for a channel — pure so the mapping is unit-testable.
pub fn endpoint_for(channel: ReleaseChannel) -> &'static str {
    match channel {
        ReleaseChannel::Stable => STABLE_ENDPOINT,
        ReleaseChannel::Beta => BETA_ENDPOINT,
    }
}

/// Managed updater state: the update found by the last successful check,
/// held so the S6/D3 "Download and install" action can consume it.
#[derive(Default)]
pub struct UpdaterState {
    pending: Mutex<Option<Update>>,
}

/// What the frontend's update state machine needs to render D3.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub notes: Option<String>,
    pub date: Option<String>,
}

/// Check the channel's manifest for a newer version. `None` means
/// up-to-date; errors (offline, 404, bad signature metadata) propagate so
/// the frontend can show the error-with-retry state.
#[tauri::command]
pub async fn check_for_update(
    app: AppHandle,
    updater: State<'_, UpdaterState>,
    channel: ReleaseChannel,
) -> Result<Option<UpdateInfo>, String> {
    let endpoint = endpoint_for(channel)
        .parse()
        .map_err(|e| format!("invalid updater endpoint: {e}"))?;
    let update = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    let info = update.as_ref().map(|u| UpdateInfo {
        version: u.version.clone(),
        notes: u.body.clone(),
        date: u.date.as_ref().map(ToString::to_string),
    });
    *updater.pending.lock().expect("updater lock") = update;
    Ok(info)
}

/// Download, verify (minisign signature against the baked-in pubkey) and
/// install the update found by [`check_for_update`]. On Windows the
/// installer exits the app by itself; elsewhere the frontend offers
/// "Restart now" (see [`restart_app`]) once this resolves.
#[tauri::command]
pub async fn download_and_install_update(updater: State<'_, UpdaterState>) -> Result<(), String> {
    let update = updater
        .pending
        .lock()
        .expect("updater lock")
        .take()
        .ok_or_else(|| "no pending update — run check_for_update first".to_string())?;
    update
        .download_and_install(|_chunk_length, _content_length| {}, || {})
        .await
        .map_err(|e| e.to_string())
}

/// Restart into the freshly installed update (macOS/Linux; Windows restarts
/// via its installer).
#[tauri::command]
pub fn restart_app(app: AppHandle) {
    tauri::process::restart(&app.env());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_map_to_their_manifests() {
        assert_eq!(endpoint_for(ReleaseChannel::Stable), STABLE_ENDPOINT);
        assert_eq!(endpoint_for(ReleaseChannel::Beta), BETA_ENDPOINT);
        // Stable resolves through GitHub's latest-release alias; beta must
        // not — that alias never points at prereleases.
        assert!(STABLE_ENDPOINT.contains("/releases/latest/download/latest.json"));
        assert!(BETA_ENDPOINT.contains("/releases/download/updates/beta.json"));
    }
}

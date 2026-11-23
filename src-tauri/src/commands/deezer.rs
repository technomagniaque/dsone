//! Tauri commands for the Deezer provider. Contract: the frontend's
//! `deezer-contract.md` (argument names are camelCase; shapes match the TIDAL
//! ones plus `provider: "deezer"`).

use std::sync::atomic::Ordering;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

use super::playback::compute_norm_gain;
use crate::deezer::models::{
    DeezerUser, DzAlbumDetail, DzAlbumPage, DzArtist, DzArtistPage, DzPlaylist, DzPlaylistPage,
    DzSearchResults, DzTrack,
};
use crate::deezer::{Quality, ResolvedStream};
use crate::tidal_api::StreamInfo;
use crate::{AppState, SoneError};

#[tauri::command(rename_all = "camelCase")]
pub async fn deezer_login(
    state: State<'_, AppState>,
    arl: String,
) -> Result<DeezerUser, SoneError> {
    state.deezer.login(&arl).await
}

const LOGIN_WINDOW: &str = "deezer-login";
const LOGIN_PAGE: &str = "https://account.deezer.com/en/login/";

/// Opens Deezer's own sign-in page in an app window (as Deezer Enhanced
/// does) and signs in with the `arl` cookie Deezer sets once the user is
/// logged in. Emits `deezer-login-success` (the user), `deezer-login-error`
/// (a message; the window stays open for another try) or
/// `deezer-login-cancelled` when the window is closed first.
#[tauri::command]
pub async fn start_deezer_login_window(app: AppHandle) -> Result<(), SoneError> {
    if let Some(existing) = app.get_webview_window(LOGIN_WINDOW) {
        let _ = existing.set_focus();
        return Ok(());
    }
    let url: tauri::Url = LOGIN_PAGE.parse().expect("valid Deezer login URL");
    let window = WebviewWindowBuilder::new(&app, LOGIN_WINDOW, WebviewUrl::External(url))
        .title("Sign in to Deezer")
        .inner_size(1024.0, 768.0)
        .center()
        .resizable(true)
        // Private session: no stale `arl` from an earlier login, and the
        // only stored copy stays dsone's encrypted one.
        .incognito(true)
        // Stay on the web: the cookie is read here, no desktop hand-off.
        .on_navigation(|url: &tauri::Url| matches!(url.scheme(), "https" | "http" | "about"))
        .build()
        .map_err(|e| SoneError::NotConfigured(format!("Failed to open Deezer sign-in: {e}")))?;

    #[cfg(target_os = "linux")]
    {
        crate::webview_proxy_auth::install_window(&window);
        use webkit2gtk::{PermissionRequestExt, WebViewExt};
        let _ = window.with_webview(|webview| {
            let wv: webkit2gtk::WebView = webview.inner();
            wv.connect_permission_request(|_wv, request| {
                request.deny();
                true
            });
        });
    }

    tauri::async_runtime::spawn(watch_login_cookie(app));
    Ok(())
}

/// Polls the login window's cookie store until Deezer sets a working `arl`.
async fn watch_login_cookie(app: AppHandle) {
    let probe: tauri::Url = "https://www.deezer.com/".parse().expect("valid URL");
    let mut tried: Option<String> = None;
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(700)).await;
        let Some(window) = app.get_webview_window(LOGIN_WINDOW) else {
            let _ = app.emit("deezer-login-cancelled", ());
            return;
        };
        let arl = match window.cookies_for_url(probe.clone()) {
            Ok(cookies) => cookies
                .into_iter()
                .find(|c| c.name() == "arl" && !c.value().is_empty())
                .map(|c| c.value().to_string()),
            Err(e) => {
                log::warn!("[deezer-login] cookie read failed: {e}");
                None
            }
        };
        // An ARL that already failed is not retried until it changes.
        let Some(arl) = arl.filter(|a| tried.as_deref() != Some(a)) else {
            continue;
        };
        tried = Some(arl.clone());
        match app.state::<AppState>().deezer.login(&arl).await {
            Ok(user) => {
                let _ = window.close();
                let _ = app.emit("deezer-login-success", &user);
                return;
            }
            Err(e) => {
                log::warn!("[deezer-login] ARL from the sign-in window rejected: {e}");
                let _ = app.emit("deezer-login-error", e.to_string());
            }
        }
    }
}

#[tauri::command]
pub async fn deezer_get_editable_playlists(
    state: State<'_, AppState>,
) -> Result<Vec<DzPlaylist>, SoneError> {
    state.deezer.editable_playlists().await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn deezer_add_tracks_to_playlist(
    state: State<'_, AppState>,
    playlist_id: u64,
    track_ids: Vec<u64>,
) -> Result<(), SoneError> {
    if track_ids.is_empty() {
        return Ok(());
    }
    state.deezer.add_to_playlist(playlist_id, &track_ids).await
}

/// Returns the new playlist's id.
#[tauri::command(rename_all = "camelCase")]
pub async fn deezer_create_playlist(
    state: State<'_, AppState>,
    title: String,
    description: String,
    is_public: bool,
    track_ids: Vec<u64>,
) -> Result<u64, SoneError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(SoneError::Parse("playlist title is empty".into()));
    }
    state
        .deezer
        .create_playlist(title, description.trim(), is_public, &track_ids)
        .await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn deezer_remove_tracks_from_playlist(
    state: State<'_, AppState>,
    playlist_id: u64,
    track_ids: Vec<u64>,
) -> Result<(), SoneError> {
    if track_ids.is_empty() {
        return Ok(());
    }
    state
        .deezer
        .remove_from_playlist(playlist_id, &track_ids)
        .await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn deezer_delete_playlist(
    state: State<'_, AppState>,
    playlist_id: u64,
) -> Result<(), SoneError> {
    state.deezer.delete_playlist(playlist_id).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn deezer_update_playlist(
    state: State<'_, AppState>,
    playlist_id: u64,
    title: String,
    description: String,
    is_public: bool,
) -> Result<(), SoneError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(SoneError::Parse("playlist title is empty".into()));
    }
    state
        .deezer
        .update_playlist(playlist_id, title, description.trim(), is_public)
        .await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn deezer_reorder_playlist(
    state: State<'_, AppState>,
    playlist_id: u64,
    track_ids: Vec<u64>,
) -> Result<(), SoneError> {
    state.deezer.reorder_playlist(playlist_id, &track_ids).await
}

#[tauri::command]
pub async fn deezer_restore_session(
    state: State<'_, AppState>,
) -> Result<Option<DeezerUser>, SoneError> {
    state.deezer.restore().await
}

#[tauri::command]
pub async fn deezer_logout(state: State<'_, AppState>) -> Result<(), SoneError> {
    state.deezer.logout();
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn deezer_search(
    state: State<'_, AppState>,
    query: String,
    limit: Option<u32>,
) -> Result<DzSearchResults, SoneError> {
    let limit = limit.unwrap_or(20).clamp(1, 100);
    state.deezer.search(&query, limit).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn deezer_get_album(
    state: State<'_, AppState>,
    album_id: u64,
) -> Result<DzAlbumPage, SoneError> {
    state.deezer.album(album_id).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn deezer_get_playlist(
    state: State<'_, AppState>,
    playlist_id: u64,
) -> Result<DzPlaylistPage, SoneError> {
    state.deezer.playlist(playlist_id).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn deezer_get_artist(
    state: State<'_, AppState>,
    artist_id: u64,
) -> Result<DzArtistPage, SoneError> {
    state.deezer.artist(artist_id).await
}

#[tauri::command]
pub async fn deezer_get_favorite_tracks(
    state: State<'_, AppState>,
) -> Result<Vec<DzTrack>, SoneError> {
    state.deezer.favorite_tracks().await
}

#[tauri::command]
pub async fn deezer_get_user_playlists(
    state: State<'_, AppState>,
) -> Result<Vec<DzPlaylist>, SoneError> {
    state.deezer.user_playlists().await
}

#[tauri::command]
pub async fn deezer_get_user_albums(
    state: State<'_, AppState>,
) -> Result<Vec<DzAlbumDetail>, SoneError> {
    state.deezer.user_albums().await
}

#[tauri::command]
pub async fn deezer_get_user_artists(
    state: State<'_, AppState>,
) -> Result<Vec<DzArtist>, SoneError> {
    state.deezer.user_artists().await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn deezer_get_charts(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> Result<DzSearchResults, SoneError> {
    let limit = limit.unwrap_or(20).clamp(1, 100);
    state.deezer.charts(limit).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn deezer_get_flow(
    state: State<'_, AppState>,
    config_id: Option<String>,
) -> Result<Vec<DzTrack>, SoneError> {
    state.deezer.flow(config_id.as_deref()).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn deezer_get_track_mix(
    state: State<'_, AppState>,
    track_id: u64,
) -> Result<Vec<DzTrack>, SoneError> {
    state.deezer.track_mix(track_id).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn deezer_get_artist_radio(
    state: State<'_, AppState>,
    artist_id: u64,
) -> Result<Vec<DzTrack>, SoneError> {
    state.deezer.artist_radio(artist_id).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn deezer_set_favorite_track(
    state: State<'_, AppState>,
    track_id: u64,
    favorite: bool,
) -> Result<(), SoneError> {
    state.deezer.set_favorite(track_id, favorite).await
}

#[tauri::command]
pub fn get_deezer_quality(state: State<'_, AppState>) -> String {
    state.deezer.quality().as_str().to_string()
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_deezer_quality(state: State<'_, AppState>, quality: String) -> Result<(), SoneError> {
    let q = Quality::parse(&quality)
        .ok_or_else(|| SoneError::Parse(format!("invalid deezer quality: {quality}")))?;
    state.deezer.set_quality(q);
    let mut settings = state.load_settings().unwrap_or_default();
    settings.deezer_quality = q.as_str().to_string();
    state.save_settings(&settings)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Playback
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct QualityFallback {
    track_id: u64,
    requested: &'static str,
    actual: &'static str,
}

/// StreamInfo for a Deezer stream. FLAC streams read their actual bit depth
/// and sample rate (16-bit/44.1 kHz up to 24-bit/192 kHz) from the leading
/// `STREAMINFO` block; 24-bit or >48 kHz masters report `HI_RES_LOSSLESS`.
/// Deezer exposes one loudness value (`GAIN`), used as track replay gain;
/// there is no album gain or peak.
fn stream_info(r: &ResolvedStream, uri: &str) -> StreamInfo {
    let (codec, bit_depth, sample_rate, quality) = match r.format {
        Quality::Flac => {
            let bd = r.bit_depth.unwrap_or(16);
            let sr = r.sample_rate.unwrap_or(44_100);
            let q = if bd > 16 || sr > 48_000 {
                "HI_RES_LOSSLESS"
            } else {
                "LOSSLESS"
            };
            ("FLAC", Some(bd), Some(sr), q)
        }
        Quality::Mp3_320 => ("MP3", None, Some(44_100), "HIGH"),
        Quality::Mp3_128 => ("MP3", None, Some(44_100), "LOW"),
    };
    StreamInfo {
        url: uri.to_string(),
        codec: Some(codec.to_string()),
        bit_depth,
        sample_rate,
        audio_quality: Some(quality.to_string()),
        audio_mode: Some("STEREO".to_string()),
        asset_presentation: Some("FULL".to_string()),
        manifest: None,
        manifest_mime_type: None,
        manifest_hash: None,
        track_id: Some(r.track_id),
        album_replay_gain: None,
        album_peak_amplitude: None,
        track_replay_gain: r.gain,
        track_peak_amplitude: None,
    }
}

/// Resolve the stream at the user's quality (falling back downward), emit
/// `deezer-quality-fallback` when Deezer served less, and compute the
/// normalization gain. Returns `(info, loopback_uri, norm_gain, rg)`.
///
/// `use_track_gain` is accepted for parity with TIDAL: Deezer only has a
/// track-level value, so album and track context use the same gain.
async fn resolve_deezer(
    app: &AppHandle,
    state: &AppState,
    track_id: u64,
    _use_track_gain: bool,
) -> Result<(StreamInfo, String, f64, f64), SoneError> {
    let requested = state.deezer.quality();
    let resolved = state
        .deezer
        .resolve_stream(track_id, &requested.cascade())
        .await?;
    if resolved.format != requested {
        log::info!(
            "[deezer] track {track_id}: requested {}, got {}",
            requested.as_str(),
            resolved.format.as_str()
        );
        let _ = app.emit(
            "deezer-quality-fallback",
            QualityFallback {
                track_id,
                requested: requested.as_str(),
                actual: resolved.format.as_str(),
            },
        );
    }
    let uri = state.deezer.loopback_url(track_id, resolved.format).await?;
    let info = stream_info(&resolved, &uri);
    let bit_perfect = state.bit_perfect.load(Ordering::Relaxed);
    let norm_gain = if state.volume_normalization.load(Ordering::Relaxed) && !bit_perfect {
        compute_norm_gain(resolved.gain, None)
    } else {
        1.0
    };
    log::debug!(
        "[deezer] track {track_id}: gain={:?} dB, bit_perfect={bit_perfect}, norm_gain={norm_gain:.3}",
        resolved.gain
    );
    Ok((info, uri, norm_gain, resolved.gain.unwrap_or(f64::NAN)))
}

/// Mirror of `play_tidal_track`. Skips the proxy advisory check
/// (`explain_block`): GStreamer only talks to the 127.0.0.1 server, and the
/// outbound CDN fetch goes through the shared proxied client. Does not write
/// `last_track_id`, which is a TIDAL id.
#[tauri::command(rename_all = "camelCase")]
pub async fn play_deezer_track(
    app: AppHandle,
    state: State<'_, AppState>,
    track_id: u64,
    use_track_gain: bool,
) -> Result<StreamInfo, SoneError> {
    let (info, uri, norm_gain, rg) =
        resolve_deezer(&app, state.inner(), track_id, use_track_gain).await?;

    state
        .last_replay_gain
        .store(rg.to_bits(), Ordering::Relaxed);
    state
        .last_peak_amplitude
        .store(f64::NAN.to_bits(), Ordering::Relaxed);

    // Gain first, so the pipeline builds at the right level (see
    // play_tidal_track).
    let player = state.audio_player.clone();
    tokio::task::spawn_blocking(move || {
        player.set_normalization_gain(norm_gain)?;
        player.play_url(&uri, None)
    })
    .await
    .map_err(|e| SoneError::Audio(e.to_string()))?
    .map_err(SoneError::Audio)?;

    Ok(info)
}

/// Mirror of `set_next_track` (gapless pre-arm).
#[tauri::command(rename_all = "camelCase")]
pub async fn set_next_deezer_track(
    app: AppHandle,
    state: State<'_, AppState>,
    track_id: u64,
    qid: String,
    use_track_gain: bool,
) -> Result<StreamInfo, SoneError> {
    let (info, uri, gain, rg) =
        resolve_deezer(&app, state.inner(), track_id, use_track_gain).await?;
    state
        .audio_player
        .set_next_track(uri, gain, track_id, qid, rg, f64::NAN, false)
        .map_err(SoneError::Audio)?;
    Ok(info)
}

// ---------------------------------------------------------------------------
// Cloud Queue sync (Android <-> dsone), listening history, lyrics & credits
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn deezer_pull_cloud_queue(
    state: State<'_, AppState>,
) -> Result<Option<crate::deezer::queue_sync::DzCloudQueueState>, SoneError> {
    state.deezer.pull_cloud_queue().await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn deezer_push_cloud_queue(
    state: State<'_, AppState>,
    input: crate::deezer::queue_sync::DzPushQueueInput,
) -> Result<crate::deezer::queue_sync::DzPushQueueResult, SoneError> {
    state.deezer.push_cloud_queue(input).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn deezer_get_history(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<DzTrack>, SoneError> {
    state.deezer.listening_history(limit.unwrap_or(50)).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn deezer_log_listen(
    state: State<'_, AppState>,
    track_id: u64,
    listened_secs: u64,
    format: Option<String>,
    is_shuffle: Option<bool>,
    repeat_mode: Option<u8>,
    context_id: Option<String>,
    context_type: Option<String>,
) -> Result<(), SoneError> {
    let fmt = format.unwrap_or_else(|| state.deezer.quality().as_str().to_string());
    state
        .deezer
        .log_listen(
            track_id,
            listened_secs,
            &fmt,
            is_shuffle.unwrap_or(false),
            repeat_mode.unwrap_or(0),
            context_id.as_deref(),
            context_type.as_deref(),
        )
        .await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn deezer_get_lyrics(
    state: State<'_, AppState>,
    track_id: u64,
) -> Result<crate::tidal_api::TidalLyrics, SoneError> {
    state.deezer.lyrics(track_id).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn deezer_get_track_credits(
    state: State<'_, AppState>,
    track_id: u64,
) -> Result<Vec<crate::tidal_api::TidalCredit>, SoneError> {
    state.deezer.track_credits(track_id).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn deezer_set_favorite_album(
    state: State<'_, AppState>,
    album_id: u64,
    favorite: bool,
) -> Result<(), SoneError> {
    if favorite {
        state.deezer.add_favorite_album(album_id).await
    } else {
        state.deezer.remove_favorite_album(album_id).await
    }
}

#[tauri::command(rename_all = "camelCase")]
pub async fn deezer_set_favorite_artist(
    state: State<'_, AppState>,
    artist_id: u64,
    favorite: bool,
) -> Result<(), SoneError> {
    if favorite {
        state.deezer.add_favorite_artist(artist_id).await
    } else {
        state.deezer.remove_favorite_artist(artist_id).await
    }
}

#[tauri::command(rename_all = "camelCase")]
pub async fn deezer_set_favorite_playlist(
    state: State<'_, AppState>,
    playlist_id: u64,
    favorite: bool,
) -> Result<(), SoneError> {
    if favorite {
        state.deezer.add_favorite_playlist(playlist_id).await
    } else {
        state.deezer.remove_favorite_playlist(playlist_id).await
    }
}

#[tauri::command(rename_all = "camelCase")]
pub async fn deezer_dislike_track(
    state: State<'_, AppState>,
    track_id: u64,
) -> Result<(), SoneError> {
    state.deezer.dislike_track(track_id).await
}

//! Native Deezer provider: session (ARL), catalogue, library and the
//! decrypting loopback stream server that feeds SONE's GStreamer pipeline.

pub mod api;
pub mod crypto;
pub mod listen_reporter;
pub mod models;
pub mod queue_sync;
pub mod stream_server;

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::crypto::Crypto;
use crate::proxy_http::ProxiedHttp;
use crate::SoneError;
use api::{GwError, Http, MediaAnswer};
use models::{
    DeezerUser, DzAlbumDetail, DzAlbumPage, DzArtist, DzArtistPage, DzPlaylist, DzPlaylistPage,
    DzSearchResults, DzTrack,
};

/// How long a resolved CDN URL is reused before the server asks again. CDN
/// URLs live longer than this; the margin covers clock skew and long pauses.
const STREAM_TTL: Duration = Duration::from_secs(4 * 60);
/// gw `song.getListData` batch size.
const LIST_BATCH: usize = 200;

/// Deezer audio formats, best first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Quality {
    Flac,
    Mp3_320,
    Mp3_128,
}

impl Quality {
    pub const ORDER: [Quality; 3] = [Quality::Flac, Quality::Mp3_320, Quality::Mp3_128];

    pub fn as_str(self) -> &'static str {
        match self {
            Quality::Flac => "FLAC",
            Quality::Mp3_320 => "MP3_320",
            Quality::Mp3_128 => "MP3_128",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ORDER.into_iter().find(|q| q.as_str() == s)
    }

    pub fn ext(self) -> &'static str {
        match self {
            Quality::Flac => "flac",
            Quality::Mp3_320 | Quality::Mp3_128 => "mp3",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Quality::Flac => "audio/flac",
            Quality::Mp3_320 | Quality::Mp3_128 => "audio/mpeg",
        }
    }

    /// The setting itself, then every lower tier.
    pub fn cascade(self) -> Vec<Quality> {
        Self::ORDER
            .into_iter()
            .filter(|q| *q as u8 >= self as u8)
            .collect()
    }

    fn filesize_key(self) -> &'static str {
        match self {
            Quality::Flac => "FILESIZE_FLAC",
            Quality::Mp3_320 => "FILESIZE_MP3_320",
            Quality::Mp3_128 => "FILESIZE_MP3_128",
        }
    }
}

/// An authenticated gw-light session. Debug is hand-written so the ARL and
/// tokens can never reach a log line through `{:?}`.
struct Session {
    arl: String,
    api_token: String,
    license_token: String,
    cookies: BTreeMap<String, String>,
    user: DeezerUser,
    /// The account's "Loved Tracks" playlist (filled through the heart, not
    /// through "Add to playlist").
    loved_playlist_id: Option<u64>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("user_id", &self.user.id)
            .field("arl", &"<redacted>")
            .finish()
    }
}

fn cookie_header(arl: &str, cookies: &BTreeMap<String, String>) -> String {
    let mut h = format!("arl={arl}");
    for (k, v) in cookies {
        if k != "arl" {
            h.push_str(&format!("; {k}={v}"));
        }
    }
    h
}

/// An ARL is a long token of URL-safe characters. Anything else would let the
/// value break out of the Cookie header.
fn validate_arl(arl: &str) -> Result<String, SoneError> {
    let arl = arl.trim();
    if arl.len() < 32
        || arl.len() > 1024
        || !arl
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.%".contains(&b))
    {
        return Err(SoneError::Api {
            status: 400,
            body: "This does not look like a Deezer ARL".into(),
        });
    }
    Ok(arl.to_string())
}

fn invalid_arl() -> SoneError {
    SoneError::Api {
        status: 401,
        body: "Deezer rejected this ARL (expired or invalid)".into(),
    }
}

/// Track-token data needed to stream one song.
#[derive(Debug, Clone)]
struct SongSource {
    id: u64,
    token: Option<String>,
    token_expire: Option<u64>,
    gain: Option<f64>,
    raw: Value,
    fallback: Option<Box<SongSource>>,
}

impl SongSource {
    fn from_gw(v: &Value) -> Option<Self> {
        Some(SongSource {
            id: models::num(v, "SNG_ID")?,
            token: models::text(v, "TRACK_TOKEN"),
            token_expire: models::num(v, "TRACK_TOKEN_EXPIRE"),
            gain: models::float(v, "GAIN").filter(|g| g.is_finite() && (-30.0..=15.0).contains(g)),
            raw: v.clone(),
            fallback: v
                .get("FALLBACK")
                .and_then(SongSource::from_gw)
                .map(Box::new),
        })
    }

    fn token_fresh(&self) -> bool {
        match (&self.token, self.token_expire) {
            (Some(_), Some(exp)) => crate::now_secs() + 60 < exp,
            (Some(_), None) => true,
            _ => false,
        }
    }

    fn filesize(&self, q: Quality) -> Option<u64> {
        models::num(&self.raw, q.filesize_key()).filter(|&s| s > 0)
    }
}

/// Stream parameters parsed from a FLAC file's leading `STREAMINFO` block
/// (RFC 9639 §8.2, first 42 bytes of the decrypted file).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlacStreamInfo {
    pub sample_rate: u32,
    pub channels: u32,
    pub bit_depth: u32,
}

/// Parse `STREAMINFO` from the first ≥42 plaintext bytes of a FLAC stream.
/// Deezer Hi-Fi delivers FLAC masters from 16-bit / 44.1 kHz up to
/// 24-bit / 192 kHz under the `FLAC` format identifier.
pub fn parse_flac_streaminfo(plain: &[u8]) -> Option<FlacStreamInfo> {
    if plain.len() < 42 || &plain[0..4] != b"fLaC" {
        return None;
    }
    let block_type = plain[4] & 0x7F;
    let block_len =
        ((plain[5] as usize) << 16) | ((plain[6] as usize) << 8) | (plain[7] as usize);
    if block_type != 0 || block_len < 34 {
        return None;
    }
    let b18 = plain[18] as u32;
    let b19 = plain[19] as u32;
    let b20 = plain[20] as u32;
    let b21 = plain[21] as u32;
    let sample_rate = (b18 << 12) | (b19 << 4) | (b20 >> 4);
    let channels = ((b20 >> 1) & 0x07) + 1;
    let bit_depth = (((b20 & 0x01) << 4) | (b21 >> 4)) + 1;
    if !(8_000..=768_000).contains(&sample_rate) || !(8..=32).contains(&bit_depth) {
        return None;
    }
    Some(FlacStreamInfo {
        sample_rate,
        channels,
        bit_depth,
    })
}

/// A CDN URL ready to be served by the loopback server.
#[derive(Debug, Clone)]
pub struct ResolvedStream {
    /// The id the user asked for (and the loopback URL carries).
    pub track_id: u64,
    /// The id whose file is streamed — differs for region fallbacks — and
    /// therefore the id the Blowfish key derives from.
    pub key_id: u64,
    pub url: String,
    pub format: Quality,
    pub size: Option<u64>,
    pub gain: Option<f64>,
    /// Probed from FLAC `STREAMINFO` (16-bit vs 24-bit, 44.1..192 kHz).
    pub bit_depth: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
    pub resolved_at: Instant,
}

pub struct DeezerClient {
    http: Http,
    session: Mutex<Option<Session>>,
    pipe_jwt: Mutex<Option<queue_sync::PipeJwt>>,
    quality: Mutex<Quality>,
    streams: Mutex<HashMap<u64, ResolvedStream>>,
    server: tokio::sync::OnceCell<stream_server::ServerHandle>,
    session_path: PathBuf,
    /// `None` only in tests, which must not touch the OS keyring.
    crypto: Option<Arc<Crypto>>,
}

impl DeezerClient {
    pub fn new(
        http: ProxiedHttp,
        crypto: Arc<Crypto>,
        session_path: PathBuf,
        quality: &str,
    ) -> Self {
        Self {
            http: Http(http),
            session: Mutex::new(None),
            pipe_jwt: Mutex::new(None),
            quality: Mutex::new(Quality::parse(quality).unwrap_or(Quality::Flac)),
            streams: Mutex::new(HashMap::new()),
            server: tokio::sync::OnceCell::new(),
            session_path,
            crypto: Some(crypto),
        }
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(http: ProxiedHttp) -> Self {
        Self {
            http: Http(http),
            session: Mutex::new(None),
            pipe_jwt: Mutex::new(None),
            quality: Mutex::new(Quality::Flac),
            streams: Mutex::new(HashMap::new()),
            server: tokio::sync::OnceCell::new(),
            session_path: PathBuf::from("/nonexistent/deezer_session.bin"),
            crypto: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn insert_stream_for_test(&self, s: ResolvedStream) {
        self.lock_streams().insert(s.track_id, s);
    }

    fn lock_session(&self) -> std::sync::MutexGuard<'_, Option<Session>> {
        self.session.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn lock_streams(&self) -> std::sync::MutexGuard<'_, HashMap<u64, ResolvedStream>> {
        self.streams.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn quality(&self) -> Quality {
        *self.quality.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn set_quality(&self, q: Quality) {
        *self.quality.lock().unwrap_or_else(|p| p.into_inner()) = q;
        // Clear cached resolved streams so changing quality (e.g. MP3 -> FLAC)
        // takes effect immediately on the next play/seek instead of serving a
        // stale lower-tier resolution from cache.
        self.lock_streams().clear();
    }

    pub fn cached_stream_format(&self, track_id: u64) -> Option<Quality> {
        self.lock_streams().get(&track_id).map(|s| s.format)
    }

    pub fn user_id(&self) -> Result<u64, SoneError> {
        self.lock_session()
            .as_ref()
            .map(|s| s.user.id)
            .ok_or(SoneError::NotAuthenticated)
    }

    pub fn is_logged_in(&self) -> bool {
        self.lock_session().is_some()
    }

    // ---- session -------------------------------------------------------

    pub async fn login(&self, arl: &str) -> Result<DeezerUser, SoneError> {
        let arl = validate_arl(arl)?;
        let session = self.open_session(arl).await?;
        let user = session.user.clone();
        self.persist_arl(&session.arl)?;
        *self.lock_session() = Some(session);
        *self.pipe_jwt.lock().unwrap_or_else(|p| p.into_inner()) = None;
        self.lock_streams().clear();
        log::info!("[deezer] signed in (user id {})", user.id);
        Ok(user)
    }

    /// `Ok(None)` when no ARL is stored, or the stored one is no longer valid
    /// (it is then forgotten). Network failures are `Err` and keep the ARL.
    pub async fn restore(&self) -> Result<Option<DeezerUser>, SoneError> {
        let Some(arl) = self.load_arl() else {
            return Ok(None);
        };
        match self.open_session(arl).await {
            Ok(session) => {
                let user = session.user.clone();
                *self.lock_session() = Some(session);
                Ok(Some(user))
            }
            Err(SoneError::Api { status: 401, .. }) => {
                log::info!("[deezer] stored ARL no longer valid; forgetting it");
                self.forget_arl();
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    pub fn logout(&self) {
        *self.lock_session() = None;
        *self.pipe_jwt.lock().unwrap_or_else(|p| p.into_inner()) = None;
        self.lock_streams().clear();
        self.forget_arl();
    }

    async fn open_session(&self, arl: String) -> Result<Session, SoneError> {
        let mut cookies = BTreeMap::new();
        let (json, set) = api::gw_call(
            &self.http,
            "deezer.getUserData",
            "null",
            &cookie_header(&arl, &cookies),
            &json!({}),
        )
        .await?;
        cookies.extend(set);
        if let Some(e) = api::gw_error(&json) {
            return Err(gw_failure("deezer.getUserData", e));
        }
        let results = json.get("results").cloned().unwrap_or(Value::Null);
        let (user, license_token) = models::gw_user(&results).ok_or_else(invalid_arl)?;
        let api_token = models::text(&results, "checkForm")
            .ok_or_else(|| SoneError::Parse("deezer: no checkForm in user data".into()))?;
        let loved_playlist_id = results
            .get("USER")
            .and_then(|u| models::num(u, "LOVEDTRACKS_ID"))
            .filter(|&id| id != 0);
        Ok(Session {
            arl,
            api_token,
            license_token,
            cookies,
            user,
            loved_playlist_id,
        })
    }

    /// Re-read user data with the current cookies: new api_token and
    /// license_token, and catches a session that expired server-side.
    async fn refresh_session(&self) -> Result<(), SoneError> {
        let arl = self
            .lock_session()
            .as_ref()
            .map(|s| s.arl.clone())
            .ok_or(SoneError::NotAuthenticated)?;
        let fresh = self.open_session(arl).await?;
        let mut guard = self.lock_session();
        if let Some(s) = guard.as_mut() {
            s.api_token = fresh.api_token;
            s.license_token = fresh.license_token;
            s.user = fresh.user;
            s.loved_playlist_id = fresh.loved_playlist_id;
            s.cookies.extend(fresh.cookies);
        }
        Ok(())
    }

    fn persist_arl(&self, arl: &str) -> Result<(), SoneError> {
        let Some(crypto) = self.crypto.as_ref() else {
            return Ok(());
        };
        let plain = serde_json::to_vec(&json!({ "arl": arl }))?;
        let enc = crypto.encrypt(&plain)?;
        std::fs::write(&self.session_path, enc)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.session_path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    fn load_arl(&self) -> Option<String> {
        let data = std::fs::read(&self.session_path).ok()?;
        if !crate::crypto::is_encrypted(&data) {
            // Only ever written encrypted; anything else is not ours.
            return None;
        }
        let plain = self.crypto.as_ref()?.decrypt(&data).ok()?;
        let v: Value = serde_json::from_slice(&plain).ok()?;
        models::text(&v, "arl")
    }

    fn forget_arl(&self) {
        if let Err(e) = std::fs::remove_file(&self.session_path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                log::warn!("[deezer] could not remove stored session: {e}");
            }
        }
    }

    // ---- gw-light --------------------------------------------------------

    /// Authenticated gw-light call returning `results`. Refreshes the
    /// api_token once if the gateway says it is stale.
    async fn gw(&self, method: &str, body: Value) -> Result<Value, SoneError> {
        for attempt in 0..2 {
            let (token, cookie) = {
                let guard = self.lock_session();
                let s = guard.as_ref().ok_or(SoneError::NotAuthenticated)?;
                (s.api_token.clone(), cookie_header(&s.arl, &s.cookies))
            };
            let (json, set) = api::gw_call(&self.http, method, &token, &cookie, &body).await?;
            if !set.is_empty() {
                if let Some(s) = self.lock_session().as_mut() {
                    s.cookies.extend(set);
                }
            }
            match api::gw_error(&json) {
                None => return Ok(json.get("results").cloned().unwrap_or(Value::Null)),
                Some(GwError::InvalidToken) if attempt == 0 => {
                    log::debug!("[deezer] {method}: api token stale, refreshing");
                    self.refresh_session().await?;
                }
                Some(e) => return Err(gw_failure(method, e)),
            }
        }
        Err(SoneError::NotAuthenticated)
    }

    async fn song_list(&self, ids: &[u64]) -> Result<Vec<Value>, SoneError> {
        let mut out = Vec::with_capacity(ids.len());
        for batch in ids.chunks(LIST_BATCH) {
            let r = self
                .gw("song.getListData", json!({ "sng_ids": batch }))
                .await?;
            if let Some(d) = r.get("data").and_then(|d| d.as_array()) {
                out.extend(d.iter().cloned());
            }
        }
        Ok(out)
    }

    async fn song_source(&self, track_id: u64) -> Result<SongSource, SoneError> {
        let r = self
            .gw("song.getData", json!({ "sng_id": track_id }))
            .await?;
        SongSource::from_gw(&r).ok_or_else(|| SoneError::Api {
            status: 404,
            body: format!("deezer track {track_id} not found"),
        })
    }

    // ---- catalogue (public API) ------------------------------------------

    pub async fn search(&self, query: &str, limit: u32) -> Result<DzSearchResults, SoneError> {
        let q = |path: &'static str| {
            let query = vec![("q", query.to_string()), ("limit", limit.to_string())];
            let http = self.http.clone();
            async move { api::public_get(&http, path, &query).await }
        };
        let (tracks, albums, artists, playlists) = tokio::join!(
            q("search/track"),
            q("search/album"),
            q("search/artist"),
            q("search/playlist")
        );
        let data = |r: Result<Value, SoneError>| -> Result<Value, SoneError> {
            Ok(r?.get("data").cloned().unwrap_or(Value::Null))
        };
        let list = |v: &Value| v.as_array().cloned().unwrap_or_default();
        Ok(DzSearchResults {
            tracks: models::public_tracks(&data(tracks)?, None),
            albums: list(&data(albums)?)
                .iter()
                .filter_map(models::public_album)
                .collect(),
            artists: list(&data(artists)?)
                .iter()
                .filter_map(models::public_artist)
                .collect(),
            playlists: list(&data(playlists)?)
                .iter()
                .filter_map(models::public_playlist)
                .collect(),
            videos: Vec::new(),
        })
    }

    pub async fn album(&self, album_id: u64) -> Result<DzAlbumPage, SoneError> {
        let raw = api::public_get(&self.http, &format!("album/{album_id}"), &[]).await?;
        let mut album = models::public_album(&raw)
            .ok_or_else(|| SoneError::Parse(format!("deezer album {album_id}")))?;
        let items =
            api::public_get_all(&self.http, &format!("album/{album_id}/tracks"), 1000).await?;
        let r = models::album_ref_of(&album);
        let tracks = models::public_tracks(&Value::Array(items), Some(&r));
        album.number_of_volumes = tracks.iter().filter_map(|t| t.volume_number).max();
        Ok(DzAlbumPage { album, tracks })
    }

    pub async fn playlist(&self, playlist_id: u64) -> Result<DzPlaylistPage, SoneError> {
        // Signed in: gw-light sees private playlists too.
        if self.is_logged_in() {
            match self
                .gw(
                    "deezer.pagePlaylist",
                    json!({
                        "playlist_id": playlist_id.to_string(),
                        "lang": "en",
                        "nb": 2000,
                        "start": 0,
                        "tab": 0,
                        "tags": true,
                        "header": true,
                    }),
                )
                .await
            {
                Ok(r) => {
                    if let Some(playlist) = r.get("DATA").and_then(models::gw_playlist) {
                        let tracks = r
                            .get("SONGS")
                            .and_then(|s| s.get("data"))
                            .map(models::gw_tracks)
                            .unwrap_or_default();
                        return Ok(DzPlaylistPage { playlist, tracks });
                    }
                }
                Err(e) => log::debug!("[deezer] pagePlaylist failed, using public API: {e}"),
            }
        }
        let raw = api::public_get(&self.http, &format!("playlist/{playlist_id}"), &[]).await?;
        let playlist = models::public_playlist(&raw)
            .ok_or_else(|| SoneError::Parse(format!("deezer playlist {playlist_id}")))?;
        let items =
            api::public_get_all(&self.http, &format!("playlist/{playlist_id}/tracks"), 5000)
                .await?;
        Ok(DzPlaylistPage {
            playlist,
            tracks: models::public_tracks(&Value::Array(items), None),
        })
    }

    pub async fn artist(&self, artist_id: u64) -> Result<DzArtistPage, SoneError> {
        let (p_artist, p_top, p_albums, p_related) = (
            format!("artist/{artist_id}"),
            format!("artist/{artist_id}/top"),
            format!("artist/{artist_id}/albums"),
            format!("artist/{artist_id}/related"),
        );
        let top_query = [("limit", "50".to_string())];
        let rel_query = [("limit", "20".to_string())];
        let (raw, top, albums, related) = tokio::join!(
            api::public_get(&self.http, &p_artist, &[]),
            api::public_get(&self.http, &p_top, &top_query),
            api::public_get_all(&self.http, &p_albums, 300),
            api::public_get(&self.http, &p_related, &rel_query),
        );
        let artist = models::public_artist(&raw?)
            .ok_or_else(|| SoneError::Parse(format!("deezer artist {artist_id}")))?;
        let top = top?;
        let albums: Vec<DzAlbumDetail> = albums?
            .iter()
            .filter_map(|a| {
                let mut a = models::public_album(a)?;
                // Discography entries omit the artist; it is this one.
                if a.artist.is_none() {
                    a.artist = Some(artist.clone());
                    a.artists = Some(vec![artist.clone()]);
                }
                Some(a)
            })
            .collect();
        let related_artists: Vec<DzArtist> = related
            .ok()
            .and_then(|v| v.get("data").and_then(|d| d.as_array()).cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(models::public_artist)
            .collect();
        let bio = if self.is_logged_in() {
            self.gw(
                "deezer.pageArtist",
                json!({ "art_id": artist_id.to_string(), "lang": "en", "tab": 0 }),
            )
            .await
            .ok()
            .and_then(|r| {
                r.get("BIO")
                    .and_then(|b| models::text(b, "BIO"))
                    .or_else(|| models::text(&r, "BIO"))
            })
        } else {
            None
        };
        Ok(DzArtistPage {
            top_tracks: models::public_tracks(top.get("data").unwrap_or(&Value::Null), None),
            artist,
            albums,
            related_artists,
            bio,
        })
    }

    /// Editorial charts (Top tracks, albums, artists, playlists) from the
    /// public Deezer API (`chart/0`).
    pub async fn charts(&self, limit: u32) -> Result<DzSearchResults, SoneError> {
        let raw = api::public_get(
            &self.http,
            "chart/0",
            &[("limit", limit.to_string())],
        )
        .await?;
        let list = |key: &str| {
            raw.get(key)
                .and_then(|v| v.get("data"))
                .and_then(|d| d.as_array())
                .cloned()
                .unwrap_or_default()
        };
        let tracks_data = raw
            .get("tracks")
            .and_then(|v| v.get("data"))
            .unwrap_or(&Value::Null);
        Ok(DzSearchResults {
            tracks: models::public_tracks(tracks_data, None),
            albums: list("albums")
                .iter()
                .filter_map(models::public_album)
                .collect(),
            artists: list("artists")
                .iter()
                .filter_map(models::public_artist)
                .collect(),
            playlists: list("playlists")
                .iter()
                .filter_map(models::public_playlist)
                .collect(),
            videos: Vec::new(),
        })
    }

    // ---- user library (gw-light) -----------------------------------------

    pub async fn favorite_tracks(&self) -> Result<Vec<DzTrack>, SoneError> {
        let r = self
            .gw(
                "song.getFavoriteIds",
                json!({ "nb": 10000, "start": 0, "checksum": null }),
            )
            .await?;
        let entries = r
            .get("data")
            .and_then(|d| d.as_array())
            .cloned()
            .unwrap_or_default();
        let added: HashMap<u64, u64> = entries
            .iter()
            .filter_map(|e| Some((models::num(e, "SNG_ID")?, models::num(e, "DATE_ADD")?)))
            .collect();
        let ids: Vec<u64> = entries
            .iter()
            .filter_map(|e| models::num(e, "SNG_ID"))
            .collect();
        let songs = self.song_list(&ids).await?;
        let by_id: HashMap<u64, DzTrack> = models::gw_tracks(&Value::Array(songs))
            .into_iter()
            .map(|t| (t.id, t))
            .collect();
        // Keep the favourites order (newest first), not the batch order.
        Ok(ids
            .iter()
            .filter_map(|id| {
                let mut t = by_id.get(id)?.clone();
                if t.date_added.is_none() {
                    t.date_added = added.get(id).copied().map(models::unix_to_iso);
                }
                Some(t)
            })
            .collect())
    }

    async fn profile_tab(&self, tab: &str) -> Result<Vec<Value>, SoneError> {
        let uid = self.user_id()?;
        let r = self
            .gw(
                "deezer.pageProfile",
                json!({ "USER_ID": uid.to_string(), "tab": tab, "nb": 2000 }),
            )
            .await?;
        Ok(r.get("TAB")
            .and_then(|t| t.get(tab))
            .and_then(|t| t.get("data"))
            .and_then(|d| d.as_array())
            .cloned()
            .unwrap_or_default())
    }

    pub async fn user_playlists(&self) -> Result<Vec<DzPlaylist>, SoneError> {
        Ok(self
            .profile_tab("playlists")
            .await?
            .iter()
            .filter_map(models::gw_playlist)
            .collect())
    }

    pub async fn user_albums(&self) -> Result<Vec<DzAlbumDetail>, SoneError> {
        Ok(self
            .profile_tab("albums")
            .await?
            .iter()
            .filter_map(models::gw_album)
            .collect())
    }

    pub async fn user_artists(&self) -> Result<Vec<DzArtist>, SoneError> {
        Ok(self
            .profile_tab("artists")
            .await?
            .iter()
            .filter_map(models::gw_artist)
            .collect())
    }

    /// User's Flow mix, optionally filtered by a Deezer Flow mood or genre
    /// `config_id` (e.g. `"chill"`, `"motivation"`, `"focus"`, `"party"`,
    /// `"melancholy"`, `"you_and_me"`, `"new_releases"`, `"genre-rock"`).
    pub async fn flow(&self, config_id: Option<&str>) -> Result<Vec<DzTrack>, SoneError> {
        let uid = self.user_id()?;
        let mut params = json!({ "user_id": uid.to_string() });
        if let Some(cid) = config_id
            .map(str::trim)
            .filter(|s| !s.is_empty() && *s != "default")
        {
            params["config_id"] = json!(cid);
        }
        let r = self.gw("radio.getUserRadio", params).await?;
        // Seen both as {data: [...]} and as a bare array.
        let list = r.get("data").cloned().unwrap_or(r);
        Ok(models::gw_tracks(&list))
    }

    /// Suggested / radio mix seeded from a single track (`song.getSearchTrackMix`).
    pub async fn track_mix(&self, track_id: u64) -> Result<Vec<DzTrack>, SoneError> {
        let r = self
            .gw(
                "song.getSearchTrackMix",
                json!({
                    "sng_id": track_id.to_string(),
                    "start_with_input_track": false,
                }),
            )
            .await?;
        let list = r.get("data").cloned().unwrap_or(r);
        Ok(models::gw_tracks(&list))
    }

    /// Radio mix seeded from an artist (`smart.getSmartRadio`).
    pub async fn artist_radio(&self, artist_id: u64) -> Result<Vec<DzTrack>, SoneError> {
        let r = self
            .gw(
                "smart.getSmartRadio",
                json!({ "art_id": artist_id.to_string() }),
            )
            .await?;
        let list = r.get("data").cloned().unwrap_or(r);
        Ok(models::gw_tracks(&list))
    }

    pub async fn set_favorite(&self, track_id: u64, favorite: bool) -> Result<(), SoneError> {
        let method = if favorite {
            "favorite_song.add"
        } else {
            "favorite_song.remove"
        };
        self.gw(method, json!({ "SNG_ID": track_id.to_string() }))
            .await?;
        Ok(())
    }

    /// Playlists the user can add tracks to: their own, minus Loved Tracks.
    pub async fn editable_playlists(&self) -> Result<Vec<DzPlaylist>, SoneError> {
        let (uid, loved) = {
            let guard = self.lock_session();
            let s = guard.as_ref().ok_or(SoneError::NotAuthenticated)?;
            (s.user.id, s.loved_playlist_id)
        };
        let all = self.user_playlists().await?;
        Ok(editable(all, uid, loved))
    }

    /// Append tracks to one of the user's playlists.
    pub async fn add_to_playlist(
        &self,
        playlist_id: u64,
        track_ids: &[u64],
    ) -> Result<(), SoneError> {
        let res = self
            .gw(
                "playlist.addSongs",
                json!({
                    "playlist_id": playlist_id.to_string(),
                    "songs": songs_payload(track_ids),
                    "offset": -1,
                    "ctxt": {
                        "id": playlist_id.to_string(),
                        "t": "playlist_page",
                    },
                }),
            )
            .await;
        match &res {
            Ok(_) => log::info!(
                "[deezer] playlist.addSongs({playlist_id}) added {} track(s)",
                track_ids.len()
            ),
            Err(e) => log::warn!(
                "[deezer] playlist.addSongs({playlist_id}) failed for {:?}: {e}",
                track_ids
            ),
        }
        res.map(|_| ())
    }

    /// Create a playlist (optionally already holding `track_ids`); returns
    /// its id.
    pub async fn create_playlist(
        &self,
        title: &str,
        description: &str,
        public: bool,
        track_ids: &[u64],
    ) -> Result<u64, SoneError> {
        let r = self
            .gw(
                "playlist.create",
                json!({
                    "title": title,
                    "description": description,
                    // STATUS: 0 public, 1 private.
                    "status": if public { 0 } else { 1 },
                    "songs": songs_payload(track_ids),
                }),
            )
            .await?;
        r.as_u64()
            .or_else(|| r.as_str().and_then(|s| s.parse().ok()))
            .ok_or_else(|| SoneError::Parse("deezer playlist.create: no playlist id".into()))
    }

    /// Remove tracks from one of the user's playlists.
    pub async fn remove_from_playlist(
        &self,
        playlist_id: u64,
        track_ids: &[u64],
    ) -> Result<(), SoneError> {
        self.gw(
            "playlist.deleteSongs",
            json!({
                "playlist_id": playlist_id.to_string(),
                "songs": songs_payload(track_ids),
            }),
        )
        .await?;
        Ok(())
    }

    /// Delete one of the user's playlists.
    pub async fn delete_playlist(&self, playlist_id: u64) -> Result<(), SoneError> {
        self.gw(
            "playlist.delete",
            json!({ "playlist_id": playlist_id.to_string() }),
        )
        .await?;
        Ok(())
    }

    /// Update a playlist's title, description and visibility.
    pub async fn update_playlist(
        &self,
        playlist_id: u64,
        title: &str,
        description: &str,
        public: bool,
    ) -> Result<(), SoneError> {
        self.gw(
            "playlist.update",
            json!({
                "playlist_id": playlist_id.to_string(),
                "title": title,
                "description": description,
                "status": if public { 0 } else { 1 },
            }),
        )
        .await?;
        Ok(())
    }

    /// Replace the track ordering of one of the user's playlists.
    pub async fn reorder_playlist(
        &self,
        playlist_id: u64,
        track_ids: &[u64],
    ) -> Result<(), SoneError> {
        let order: Vec<String> = track_ids.iter().map(|id| id.to_string()).collect();
        self.gw(
            "playlist.updateOrder",
            json!({
                "playlist_id": playlist_id.to_string(),
                "position": "0",
                "order": order,
            }),
        )
        .await?;
        Ok(())
    }

    // ---- cloud queue sync & play reporting (Android <-> dsone) -------------

    async fn pipe_jwt(&self) -> Result<queue_sync::PipeJwt, SoneError> {
        {
            let guard = self.pipe_jwt.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(cached) = guard.as_ref().filter(|j| j.is_fresh()) {
                return Ok(cached.clone());
            }
        }
        let arl = self
            .lock_session()
            .as_ref()
            .map(|s| s.arl.clone())
            .ok_or(SoneError::NotAuthenticated)?;
        let jwt = queue_sync::fetch_pipe_jwt(&self.http, &arl).await?;
        *self.pipe_jwt.lock().unwrap_or_else(|p| p.into_inner()) = Some(jwt.clone());
        Ok(jwt)
    }

    fn invalidate_pipe_jwt(&self) {
        *self.pipe_jwt.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }

    /// Pull the synchronized Cloud Queue state from Deezer's Pipe GraphQL API
    /// (shared with Deezer Android, iOS, Web and Desktop) and hydrate the tracks.
    pub async fn pull_cloud_queue(
        &self,
    ) -> Result<Option<queue_sync::DzCloudQueueState>, SoneError> {
        let mut jwt = self.pipe_jwt().await?;
        let raw = match queue_sync::pull_raw_queue(&self.http, &jwt.token).await {
            Err(SoneError::NotAuthenticated) => {
                self.invalidate_pipe_jwt();
                jwt = self.pipe_jwt().await?;
                queue_sync::pull_raw_queue(&self.http, &jwt.token).await?
            }
            other => other?,
        };
        let Some(parsed) = raw else {
            return Ok(None);
        };
        if parsed.track_ids.is_empty() {
            return Ok(None);
        }
        let songs = self.song_list(&parsed.track_ids).await?;
        let by_id: HashMap<u64, DzTrack> = models::gw_tracks(&Value::Array(songs))
            .into_iter()
            .map(|t| (t.id, t))
            .collect();
        let mut tracks = Vec::with_capacity(parsed.track_ids.len());
        let mut mapped_pos = 0usize;
        for (i, id) in parsed.track_ids.iter().enumerate() {
            if let Some(t) = by_id.get(id) {
                if i == parsed.position {
                    mapped_pos = tracks.len();
                }
                tracks.push(t.clone());
            }
        }
        if tracks.is_empty() {
            return Ok(None);
        }
        let position = mapped_pos.min(tracks.len() - 1);
        Ok(Some(queue_sync::DzCloudQueueState {
            tracks,
            position,
            repeat_mode: parsed.repeat_mode,
            shuffled: parsed.shuffled,
            version: parsed.version,
            updated_at: parsed.updated_at,
            local_queue_id: parsed.local_queue_id,
            source_user_agent: parsed.source_user_agent,
        }))
    }

    /// Push dsone's active Deezer queue & current track index to Deezer Cloud Queue.
    pub async fn push_cloud_queue(
        &self,
        input: queue_sync::DzPushQueueInput,
    ) -> Result<queue_sync::DzPushQueueResult, SoneError> {
        let mut jwt = self.pipe_jwt().await?;
        match queue_sync::push_queue_state(&self.http, &jwt, &input).await {
            Err(SoneError::NotAuthenticated) => {
                self.invalidate_pipe_jwt();
                jwt = self.pipe_jwt().await?;
                queue_sync::push_queue_state(&self.http, &jwt, &input).await
            }
            other => other,
        }
    }

    /// Cross-device listening history (`user.getSongsHistory`).
    pub async fn listening_history(&self, limit: usize) -> Result<Vec<DzTrack>, SoneError> {
        let nb = limit.clamp(1, 200);
        let r = self
            .gw("user.getSongsHistory", json!({ "nb": nb, "start": 0 }))
            .await?;
        let list = r.get("data").cloned().unwrap_or(r);
        Ok(models::gw_tracks(&list))
    }

    /// Report a played Deezer track via `log.listen` so Deezer's listening
    /// history, Flow recommendations, and Android recently-played stay in sync.
    pub async fn log_listen(
        &self,
        track_id: u64,
        listened_secs: u64,
        format: &str,
        is_shuffle: bool,
        repeat_mode: u8,
        context_id: Option<&str>,
        context_type: Option<&str>,
    ) -> Result<(), SoneError> {
        if !self.is_logged_in() {
            return Ok(());
        }
        let mut params = json!({
            "media": {
                "id": track_id.to_string(),
                "type": "song",
                "format": format,
            },
            "type": 0,
            "stat": { "seek": 0, "pause": 0, "sync": 0 },
            "lt": listened_secs,
            "ctxt": {
                "id": context_id.unwrap_or("0"),
                "t": context_type.unwrap_or("playlist_page"),
            },
            "dev": { "v": "7.1.140", "t": 0 },
            "ts_listen": crate::now_secs(),
            "is_shuffle": is_shuffle,
            "stream_id": uuid::Uuid::new_v4().to_string(),
        });
        match repeat_mode {
            1 => params["repeat_type"] = json!("repeat_all"),
            2 => params["repeat_type"] = json!("repeat_one"),
            _ => {}
        }
        self.gw("log.listen", json!({ "params": params })).await?;
        Ok(())
    }

    /// Low-level `log.listen` sender used by `DeezerListenReporter`.
    pub async fn send_log_listen_raw(&self, body: Value) -> Result<(), SoneError> {
        if !self.is_logged_in() {
            return Ok(());
        }
        self.gw("log.listen", body).await?;
        Ok(())
    }

    /// Tell Deezer's Flow & recommendation algorithm not to recommend `track_id`.
    pub async fn dislike_track(&self, track_id: u64) -> Result<(), SoneError> {
        self.gw(
            "favorite_dislike.add",
            json!({
                "ID": track_id,
                "TYPE": "song",
            }),
        )
        .await?;
        log::info!("[deezer] disliked track {track_id} (excluded from Flow/recommendations)");
        Ok(())
    }

    // ---- lyrics, credits & library favorites (gw-light) --------------------

    /// Fetch plain + synced LRC lyrics for a Deezer track (`song.getLyrics`).
    pub async fn lyrics(&self, track_id: u64) -> Result<crate::tidal_api::TidalLyrics, SoneError> {
        let r = self
            .gw("song.getLyrics", json!({ "sng_id": track_id.to_string() }))
            .await?;
        let lyrics_text = models::text(&r, "LYRICS_TEXT").filter(|s| !s.trim().is_empty());
        let subtitles = parse_deezer_sync_lyrics(r.get("LYRICS_SYNC_JSON"));
        if lyrics_text.is_none() && subtitles.is_none() {
            return Err(SoneError::Api {
                status: 404,
                body: format!("No lyrics found for Deezer track {track_id}"),
            });
        }
        let is_rtl = r
            .get("IS_RIGHT_TO_LEFT")
            .and_then(|v| v.as_bool().or_else(|| v.as_u64().map(|n| n != 0)))
            .unwrap_or(false);
        Ok(crate::tidal_api::TidalLyrics {
            track_id: Some(track_id),
            lyrics_provider: models::text(&r, "LYRICS_COPYRIGHTS").or_else(|| Some("Deezer".into())),
            provider_commontrack_id: None,
            provider_lyrics_id: models::text(&r, "LYRICS_ID"),
            lyrics: lyrics_text,
            subtitles,
            is_right_to_left: is_rtl,
        })
    }

    /// Fetch track credits from `song.getData` (`SNG_CONTRIBUTORS`).
    pub async fn track_credits(
        &self,
        track_id: u64,
    ) -> Result<Vec<crate::tidal_api::TidalCredit>, SoneError> {
        let r = self
            .gw("song.getData", json!({ "sng_id": track_id.to_string() }))
            .await?;
        Ok(parse_deezer_contributors(r.get("SNG_CONTRIBUTORS")))
    }

    pub async fn add_favorite_album(&self, album_id: u64) -> Result<(), SoneError> {
        self.gw("album.addFavorite", json!({ "ALB_ID": album_id.to_string() }))
            .await?;
        Ok(())
    }

    pub async fn remove_favorite_album(&self, album_id: u64) -> Result<(), SoneError> {
        self.gw("album.deleteFavorite", json!({ "ALB_ID": album_id.to_string() }))
            .await?;
        Ok(())
    }

    pub async fn add_favorite_artist(&self, artist_id: u64) -> Result<(), SoneError> {
        self.gw(
            "artist.addFavorite",
            json!({ "ART_ID": artist_id.to_string() }),
        )
        .await?;
        Ok(())
    }

    pub async fn remove_favorite_artist(&self, artist_id: u64) -> Result<(), SoneError> {
        self.gw(
            "artist.deleteFavorite",
            json!({ "ART_ID": artist_id.to_string() }),
        )
        .await?;
        Ok(())
    }

    pub async fn add_favorite_playlist(&self, playlist_id: u64) -> Result<(), SoneError> {
        self.gw(
            "playlist.addFavorite",
            json!({ "PARENT_PLAYLIST_ID": playlist_id.to_string() }),
        )
        .await?;
        Ok(())
    }

    pub async fn remove_favorite_playlist(&self, playlist_id: u64) -> Result<(), SoneError> {
        self.gw(
            "playlist.deleteFavorite",
            json!({ "PARENT_PLAYLIST_ID": playlist_id.to_string() }),
        )
        .await?;
        Ok(())
    }

    // ---- streaming ---------------------------------------------------------

    /// Best-effort 2048-byte ranged probe to read the FLAC `STREAMINFO` header
    /// (16-bit/44.1 kHz vs 24-bit/up to 192 kHz) before playback starts.
    async fn probe_flac_streaminfo(&self, url: &str, key_id: u64) -> Option<FlacStreamInfo> {
        let end = crypto::CHUNK_SIZE - 1;
        let url = url.to_string();
        let resp = self
            .http
            .send(|c| {
                c.get(&url)
                    .header(reqwest::header::RANGE, format!("bytes=0-{end}"))
                    .timeout(Duration::from_secs(3))
            })
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let bytes = resp.bytes().await.ok()?;
        let slice = &bytes[..bytes.len().min(crypto::CHUNK_SIZE)];
        let mut dec = crypto::StripeDecryptor::new(crypto::track_key(key_id), 0);
        let mut plain = dec.feed(slice);
        plain.extend(dec.finish());
        parse_flac_streaminfo(&plain)
    }

    /// Record FLAC stream parameters discovered while streaming chunk 0.
    pub fn record_flac_streaminfo(&self, track_id: u64, info: FlacStreamInfo) {
        if let Some(entry) = self.lock_streams().get_mut(&track_id) {
            entry.bit_depth = Some(info.bit_depth);
            entry.sample_rate = Some(info.sample_rate);
            entry.channels = Some(info.channels);
        }
    }

    /// Resolve a CDN URL for `track_id`, trying `formats` in order (Deezer
    /// picks the first it can serve). Refreshes expired track/license tokens and
    /// walks the region fallback chain — preferring any candidate that offers
    /// FLAC when FLAC is requested before stepping down to MP3.
    pub async fn resolve_stream(
        &self,
        track_id: u64,
        formats: &[Quality],
    ) -> Result<ResolvedStream, SoneError> {
        let mut license = self
            .lock_session()
            .as_ref()
            .map(|s| s.license_token.clone())
            .ok_or(SoneError::NotAuthenticated)?;
        let names: Vec<&str> = formats.iter().map(|q| q.as_str()).collect();
        let wants_flac = formats.first() == Some(&Quality::Flac);

        // Flatten the primary song + any region fallback chain.
        let mut chain = Vec::new();
        let mut cur = Some(self.song_source(track_id).await?);
        while let Some(mut s) = cur.take() {
            cur = s.fallback.take().map(|b| *b);
            chain.push(s);
        }

        // If FLAC is requested and a fallback edition advertises FILESIZE_FLAC
        // while the primary does not, try the FLAC-capable edition first.
        if wants_flac && chain.len() > 1 && chain[0].filesize(Quality::Flac).is_none() {
            if let Some(flac_idx) = chain
                .iter()
                .position(|s| s.filesize(Quality::Flac).is_some())
            {
                chain.swap(0, flac_idx);
            }
        }

        let mut reason = String::from("no stream");
        let mut license_refreshed = false;
        let mut best_fallback: Option<ResolvedStream> = None;

        for mut song in chain {
            if !song.token_fresh() && song.id != track_id {
                song = self.song_source(song.id).await?;
            }
            let mut token_refreshed = false;
            loop {
                let Some(token) = song.token.clone() else {
                    reason = "no track token".into();
                    break;
                };
                match api::get_url(&self.http, &license, &token, &names).await? {
                    MediaAnswer::Url { format, url } => {
                        let format = Quality::parse(&format).ok_or_else(|| {
                            SoneError::Parse(format!("deezer: unexpected format {format}"))
                        })?;
                        let flac_info = if format == Quality::Flac {
                            self.probe_flac_streaminfo(&url, song.id).await
                        } else {
                            None
                        };
                        let resolved = ResolvedStream {
                            track_id,
                            key_id: song.id,
                            url,
                            format,
                            size: song.filesize(format),
                            gain: song.gain,
                            bit_depth: flac_info.map(|i| i.bit_depth),
                            sample_rate: flac_info.map(|i| i.sample_rate),
                            channels: flac_info.map(|i| i.channels),
                            resolved_at: Instant::now(),
                        };
                        log::info!(
                            "[deezer] track {track_id} (key_id={}): resolved {} (filesize_flac={:?}, filesize_320={:?}, bit_depth={:?}, rate={:?})",
                            song.id,
                            format.as_str(),
                            song.filesize(Quality::Flac),
                            song.filesize(Quality::Mp3_320),
                            resolved.bit_depth,
                            resolved.sample_rate
                        );
                        // If we wanted FLAC and got it (or didn't ask for FLAC), return immediately.
                        if !wants_flac || format == Quality::Flac {
                            self.lock_streams().insert(track_id, resolved.clone());
                            return Ok(resolved);
                        }
                        // Otherwise keep the best non-FLAC candidate and still check remaining fallbacks for FLAC.
                        if best_fallback.is_none() {
                            best_fallback = Some(resolved);
                        }
                        break;
                    }
                    MediaAnswer::LicenseExpired if !license_refreshed => {
                        license_refreshed = true;
                        log::info!("[deezer] license_token expired/unauthorized, refreshing session");
                        self.refresh_session().await?;
                        license = self
                            .lock_session()
                            .as_ref()
                            .map(|s| s.license_token.clone())
                            .ok_or(SoneError::NotAuthenticated)?;
                    }
                    MediaAnswer::LicenseExpired => {
                        reason = "license token expired".into();
                        break;
                    }
                    MediaAnswer::TokenExpired if !token_refreshed => {
                        token_refreshed = true;
                        song = self.song_source(song.id).await?;
                    }
                    MediaAnswer::TokenExpired => {
                        reason = "track token expired".into();
                        break;
                    }
                    MediaAnswer::Unavailable(why) => {
                        reason = why;
                        break;
                    }
                }
            }
        }
        if let Some(resolved) = best_fallback {
            self.lock_streams().insert(track_id, resolved.clone());
            return Ok(resolved);
        }
        Err(SoneError::Api {
            status: 404,
            body: format!("Deezer track {track_id} is not streamable ({reason})"),
        })
    }

    /// Source for the stream server, for the container `ext` the URL names:
    /// the cached resolution while fresh, else a new one pinned to the
    /// cached format. The file being served must never change format between
    /// two range requests (a seek), whatever the quality setting says now.
    pub async fn stream_source(
        &self,
        track_id: u64,
        ext: &str,
    ) -> Result<ResolvedStream, SoneError> {
        let cached = self
            .lock_streams()
            .get(&track_id)
            .filter(|s| s.format.ext() == ext)
            .cloned();
        let formats = match cached {
            Some(s) if s.resolved_at.elapsed() < STREAM_TTL => return Ok(s),
            Some(s) => vec![s.format],
            None if ext == "flac" => vec![Quality::Flac],
            None => vec![Quality::Mp3_320, Quality::Mp3_128],
        };
        self.resolve_stream(track_id, &formats).await
    }

    /// Drop a cached URL the CDN refused.
    pub fn invalidate_stream(&self, track_id: u64) {
        self.lock_streams().remove(&track_id);
    }

    /// The loopback URL for a resolved stream, starting the server on first
    /// use.
    pub async fn loopback_url(
        self: &Arc<Self>,
        track_id: u64,
        format: Quality,
    ) -> Result<String, SoneError> {
        let server = self.ensure_server().await?;
        Ok(server.url_for(track_id, format))
    }

    pub async fn ensure_server(
        self: &Arc<Self>,
    ) -> Result<&stream_server::ServerHandle, SoneError> {
        self.server
            .get_or_try_init(|| stream_server::start(Arc::clone(self)))
            .await
    }

    pub(crate) fn http(&self) -> &Http {
        &self.http
    }
}

/// Convert Deezer's `LYRICS_SYNC_JSON` array (`[{ "lrc_timestamp": "[00:12.34]", "line": "..." }]`
/// or `{ "milliseconds": "12340", "line": "..." }`) into standard LRC text for SONE's lyrics parser.
fn parse_deezer_sync_lyrics(val: Option<&Value>) -> Option<String> {
    let arr = val?.as_array()?;
    let mut lines = Vec::new();
    for entry in arr {
        let line = entry.get("line").and_then(Value::as_str).unwrap_or("");
        let ts = if let Some(lrc) = entry
            .get("lrc_timestamp")
            .and_then(Value::as_str)
            .filter(|s| s.starts_with('[') && s.ends_with(']'))
        {
            lrc.to_string()
        } else if let Some(ms) = entry
            .get("milliseconds")
            .and_then(|v| {
                v.as_u64()
                    .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
            })
        {
            let mins = ms / 60_000;
            let secs = (ms % 60_000) / 1_000;
            let cs = (ms % 1_000) / 10;
            format!("[{mins:02}:{secs:02}.{cs:02}]")
        } else {
            continue;
        };
        lines.push(format!("{ts} {line}"));
    }
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// Convert Deezer's `SNG_CONTRIBUTORS` object (`{ "composer": ["Name"], "producer": ["Name"] }`)
/// into `Vec<TidalCredit>` with title-cased role names.
fn parse_deezer_contributors(val: Option<&Value>) -> Vec<crate::tidal_api::TidalCredit> {
    let Some(obj) = val.and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (role, names) in obj {
        let Some(arr) = names.as_array() else {
            continue;
        };
        let contributors: Vec<crate::tidal_api::TidalContributor> = arr
            .iter()
            .filter_map(|v| v.as_str().map(str::trim))
            .filter(|s| !s.is_empty())
            .map(|name| crate::tidal_api::TidalContributor {
                name: name.to_string(),
                id: None,
            })
            .collect();
        if contributors.is_empty() {
            continue;
        }
        let credit_type = role
            .split('_')
            .filter(|w| !w.is_empty())
            .map(|w| {
                let mut chars = w.chars();
                match chars.next() {
                    Some(c) => format!("{}{}", c.to_uppercase(), chars.as_str()),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        out.push(crate::tidal_api::TidalCredit {
            credit_type,
            contributors,
        });
    }
    out
}

/// gw-light's song list: `[[id, 0], ...]` (the second value is the song's
/// position hint, always 0 when appending).
fn songs_payload(track_ids: &[u64]) -> Value {
    Value::Array(
        track_ids
            .iter()
            .map(|id| json!([id.to_string(), 0]))
            .collect(),
    )
}

fn editable(all: Vec<DzPlaylist>, uid: u64, loved: Option<u64>) -> Vec<DzPlaylist> {
    all.into_iter()
        .filter(|p| p.creator.as_ref().is_some_and(|c| c.id == uid))
        .filter(|p| loved.is_none_or(|id| p.uuid != id.to_string()))
        .collect()
}

fn gw_failure(method: &str, e: GwError) -> SoneError {
    match e {
        GwError::InvalidToken => SoneError::NotAuthenticated,
        GwError::Other(msg) => {
            // The message is Deezer's error object, which names the failure
            // (e.g. DATA_ERROR) and carries no account data.
            SoneError::Api {
                status: 400,
                body: format!("deezer {method}: {msg}"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deezer_quality_cascade_starts_at_setting() {
        assert_eq!(
            Quality::Flac.cascade(),
            vec![Quality::Flac, Quality::Mp3_320, Quality::Mp3_128]
        );
        assert_eq!(
            Quality::Mp3_320.cascade(),
            vec![Quality::Mp3_320, Quality::Mp3_128]
        );
        assert_eq!(Quality::Mp3_128.cascade(), vec![Quality::Mp3_128]);
        assert_eq!(Quality::parse("MP3_320"), Some(Quality::Mp3_320));
        assert_eq!(Quality::parse("flac"), None);
    }

    #[test]
    fn deezer_cookie_header_and_arl_validation() {
        let mut c = BTreeMap::new();
        c.insert("sid".to_string(), "fr123".to_string());
        assert_eq!(cookie_header("abc", &c), "arl=abc; sid=fr123");
        let arl = "a".repeat(192);
        assert_eq!(validate_arl(&format!("  {arl}\n")).unwrap(), arl);
        assert!(validate_arl("short").is_err());
        assert!(validate_arl(&format!("{arl}; sid=evil")).is_err());
    }

    #[test]
    fn deezer_session_debug_redacts_arl() {
        let s = Session {
            arl: "SECRETARL".into(),
            api_token: "tok".into(),
            license_token: "lic".into(),
            cookies: BTreeMap::new(),
            user: DeezerUser {
                id: 1,
                name: "x".into(),
                picture: None,
                can_stream_lossless: false,
                can_stream_hq: false,
                country: None,
            },
            loved_playlist_id: None,
        };
        let d = format!("{s:?}");
        assert!(!d.contains("SECRETARL") && !d.contains("lic") && !d.contains("tok"));
    }

    #[test]
    fn deezer_playlist_songs_payload_and_editable_filter() {
        assert_eq!(songs_payload(&[3, 42]), json!([["3", 0], ["42", 0]]));
        let pl = |id: u64, owner: u64| {
            models::gw_playlist(&json!({
                "PLAYLIST_ID": id.to_string(), "TITLE": format!("p{id}"),
                "PARENT_USER_ID": owner.to_string()
            }))
            .unwrap()
        };
        let all = vec![pl(1, 7), pl(2, 9), pl(3, 7)];
        let ids = |v: Vec<DzPlaylist>| v.into_iter().map(|p| p.uuid).collect::<Vec<_>>();
        // Someone else's playlist and the Loved Tracks one are left out.
        assert_eq!(ids(editable(all.clone(), 7, Some(3))), vec!["1"]);
        assert_eq!(ids(editable(all, 7, None)), vec!["1", "3"]);
    }

    #[test]
    fn deezer_song_source_reads_fallback_and_sizes() {
        let v = json!({
            "SNG_ID": "10", "TRACK_TOKEN": "t", "TRACK_TOKEN_EXPIRE": 1,
            "FILESIZE_FLAC": "0", "FILESIZE_MP3_320": "9000000", "GAIN": "-8.5",
            "FALLBACK": {"SNG_ID": "11", "TRACK_TOKEN": "t2"}
        });
        let s = SongSource::from_gw(&v).unwrap();
        assert!(!s.token_fresh(), "expired in 1970");
        assert_eq!(s.filesize(Quality::Flac), None);
        assert_eq!(s.filesize(Quality::Mp3_320), Some(9_000_000));
        assert_eq!(s.gain, Some(-8.5));
        assert_eq!(s.fallback.as_ref().unwrap().id, 11);
        assert!(s.fallback.as_ref().unwrap().token_fresh());
    }

    fn build_flac_header(sample_rate: u32, channels: u32, bit_depth: u32) -> Vec<u8> {
        let mut buf = vec![0u8; 42];
        buf[0..4].copy_from_slice(b"fLaC");
        buf[4] = 0x00; // STREAMINFO block (type 0)
        buf[5] = 0;
        buf[6] = 0;
        buf[7] = 34; // block length = 34 bytes
        let ch_bits = (channels - 1) & 0x07;
        let bps_bits = (bit_depth - 1) & 0x1F;
        buf[18] = ((sample_rate >> 12) & 0xFF) as u8;
        buf[19] = ((sample_rate >> 4) & 0xFF) as u8;
        buf[20] = (((sample_rate & 0x0F) << 4) | (ch_bits << 1) | (bps_bits >> 4)) as u8;
        buf[21] = ((bps_bits & 0x0F) << 4) as u8;
        buf
    }

    #[test]
    fn deezer_flac_streaminfo_parses_16_44k_and_24_192k() {
        let cd = build_flac_header(44_100, 2, 16);
        assert_eq!(
            parse_flac_streaminfo(&cd),
            Some(FlacStreamInfo {
                sample_rate: 44_100,
                channels: 2,
                bit_depth: 16,
            })
        );

        let hires_96 = build_flac_header(96_000, 2, 24);
        assert_eq!(
            parse_flac_streaminfo(&hires_96),
            Some(FlacStreamInfo {
                sample_rate: 96_000,
                channels: 2,
                bit_depth: 24,
            })
        );

        let hires_192 = build_flac_header(192_000, 2, 24);
        assert_eq!(
            parse_flac_streaminfo(&hires_192),
            Some(FlacStreamInfo {
                sample_rate: 192_000,
                channels: 2,
                bit_depth: 24,
            })
        );

        assert_eq!(parse_flac_streaminfo(b"ID3notflac"), None);
    }

    #[test]
    fn deezer_sync_lyrics_converts_lrc_and_milliseconds() {
        let sync = json!([
            { "lrc_timestamp": "[00:05.50]", "line": "First line" },
            { "milliseconds": "65430", "line": "Second line" },
            { "line": "No timestamp skipped" }
        ]);
        let lrc = parse_deezer_sync_lyrics(Some(&sync)).unwrap();
        assert_eq!(lrc, "[00:05.50] First line\n[01:05.43] Second line");
        assert_eq!(parse_deezer_sync_lyrics(None), None);
    }

    #[test]
    fn deezer_contributors_maps_roles_to_credits() {
        let raw = json!({
            "main_artist": ["Daft Punk"],
            "composer": ["Thomas Bangalter", "Guy-Manuel de Homem-Christo"],
            "empty_role": []
        });
        let credits = parse_deezer_contributors(Some(&raw));
        assert_eq!(credits.len(), 2);
        let composer = credits
            .iter()
            .find(|c| c.credit_type == "Composer")
            .expect("Composer credit");
        assert_eq!(composer.contributors.len(), 2);
        assert_eq!(composer.contributors[0].name, "Thomas Bangalter");
    }
}

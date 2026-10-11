//! Native Deezer provider: session (ARL), catalogue, library and the
//! decrypting loopback stream server that feeds SONE's GStreamer pipeline.

pub mod api;
pub mod crypto;
pub mod models;
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
    DeezerUser, DzAlbumDetail, DzAlbumPage, DzArtistPage, DzPlaylist, DzPlaylistPage,
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
            gain: models::float(v, "GAIN"),
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
    pub resolved_at: Instant,
}

pub struct DeezerClient {
    http: Http,
    session: Mutex<Option<Session>>,
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
        Ok(Session {
            arl,
            api_token,
            license_token,
            cookies,
            user,
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
        let (p_artist, p_top, p_albums) = (
            format!("artist/{artist_id}"),
            format!("artist/{artist_id}/top"),
            format!("artist/{artist_id}/albums"),
        );
        let top_query = [("limit", "25".to_string())];
        let (raw, top, albums) = tokio::join!(
            api::public_get(&self.http, &p_artist, &[]),
            api::public_get(&self.http, &p_top, &top_query),
            api::public_get_all(&self.http, &p_albums, 300),
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
        Ok(DzArtistPage {
            top_tracks: models::public_tracks(top.get("data").unwrap_or(&Value::Null), None),
            artist,
            albums,
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

    pub async fn flow(&self) -> Result<Vec<DzTrack>, SoneError> {
        let uid = self.user_id()?;
        let r = self
            .gw("radio.getUserRadio", json!({ "user_id": uid.to_string() }))
            .await?;
        // Seen both as {data: [...]} and as a bare array.
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

    // ---- streaming ---------------------------------------------------------

    /// Resolve a CDN URL for `track_id`, trying `formats` in order (Deezer
    /// picks the first it can serve). Refreshes an expired track token and
    /// walks the region fallback chain. Caches the result for the server.
    pub async fn resolve_stream(
        &self,
        track_id: u64,
        formats: &[Quality],
    ) -> Result<ResolvedStream, SoneError> {
        let license = self
            .lock_session()
            .as_ref()
            .map(|s| s.license_token.clone())
            .ok_or(SoneError::NotAuthenticated)?;
        let names: Vec<&str> = formats.iter().map(|q| q.as_str()).collect();
        let mut candidate = Some(self.song_source(track_id).await?);
        let mut reason = String::from("no stream");
        while let Some(mut song) = candidate.take() {
            if !song.token_fresh() && song.id != track_id {
                song = self.song_source(song.id).await?;
            }
            let mut refreshed = false;
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
                        let resolved = ResolvedStream {
                            track_id,
                            key_id: song.id,
                            url,
                            format,
                            size: song.filesize(format),
                            gain: song.gain,
                            resolved_at: Instant::now(),
                        };
                        self.lock_streams().insert(track_id, resolved.clone());
                        return Ok(resolved);
                    }
                    MediaAnswer::TokenExpired if !refreshed => {
                        refreshed = true;
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
            candidate = song.fallback.map(|b| *b);
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
        };
        let d = format!("{s:?}");
        assert!(!d.contains("SECRETARL") && !d.contains("lic") && !d.contains("tok"));
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
}

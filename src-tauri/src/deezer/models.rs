//! Deezer payloads mapped onto SONE's serialized shapes.
//!
//! The frontend renders TIDAL and Deezer objects with the same components, so
//! every type here serializes exactly like its `tidal_api` counterpart
//! (camelCase, `None` as `null`) plus `"provider": "deezer"`. Mapping works on
//! `serde_json::Value` rather than typed structs: Deezer's two APIs disagree on
//! types (gw-light sends most numbers as strings), and one odd field must not
//! drop a whole page of results.

use serde::Serialize;
use serde_json::Value;

pub const PROVIDER: &str = "deezer";

const IMAGE_HOST: &str = "https://cdn-images.dzcdn.net/images";

#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DzArtist {
    pub id: u64,
    pub name: String,
    pub picture: Option<String>,
    /// "MAIN" | "FEATURED", like TIDAL's embedded artist refs.
    #[serde(rename = "type")]
    pub artist_type: Option<String>,
    pub provider: &'static str,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DzAlbumRef {
    pub id: u64,
    pub title: String,
    pub cover: Option<String>,
    pub release_date: Option<String>,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DzTrack {
    pub id: u64,
    pub title: String,
    pub duration: u32,
    pub version: Option<String>,
    pub artist: Option<DzArtist>,
    pub artists: Option<Vec<DzArtist>>,
    pub album: Option<DzAlbumRef>,
    pub audio_quality: Option<String>,
    pub track_number: Option<u32>,
    pub volume_number: Option<u32>,
    pub date_added: Option<String>,
    pub isrc: Option<String>,
    pub explicit: Option<bool>,
    pub replay_gain: Option<f64>,
    pub peak: Option<f64>,
    pub url: Option<String>,
    pub stream_ready: Option<bool>,
    pub allow_streaming: Option<bool>,
    pub provider: &'static str,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DzAlbumDetail {
    pub id: u64,
    pub title: String,
    pub version: Option<String>,
    pub cover: Option<String>,
    pub artist: Option<DzArtist>,
    pub artists: Option<Vec<DzArtist>>,
    pub number_of_tracks: Option<u32>,
    pub number_of_videos: Option<u32>,
    pub number_of_volumes: Option<u32>,
    pub duration: Option<u32>,
    pub release_date: Option<String>,
    pub upc: Option<String>,
    /// "ALBUM" | "EP" | "SINGLE" — serialized as `type`, like `TidalAlbumDetail`.
    #[serde(rename = "type")]
    pub album_type: Option<String>,
    pub copyright: Option<String>,
    pub explicit: Option<bool>,
    pub url: Option<String>,
    pub audio_quality: Option<String>,
    pub provider: &'static str,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct DzPlaylistCreator {
    pub id: u64,
    pub name: Option<String>,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DzPlaylist {
    /// The numeric Deezer id as a string (the frontend keys playlists by uuid).
    pub uuid: String,
    pub title: String,
    pub description: Option<String>,
    pub image: Option<String>,
    pub square_image: Option<String>,
    pub number_of_tracks: Option<u32>,
    pub number_of_videos: Option<u32>,
    pub creator: Option<DzPlaylistCreator>,
    pub playlist_type: Option<String>,
    pub duration: Option<u32>,
    pub last_updated: Option<String>,
    pub access_type: Option<String>,
    pub provider: &'static str,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DzSearchResults {
    pub artists: Vec<DzArtist>,
    pub albums: Vec<DzAlbumDetail>,
    pub tracks: Vec<DzTrack>,
    pub playlists: Vec<DzPlaylist>,
    /// Always empty; present because the frontend type requires it.
    pub videos: Vec<Value>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DzAlbumPage {
    pub album: DzAlbumDetail,
    pub tracks: Vec<DzTrack>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DzPlaylistPage {
    pub playlist: DzPlaylist,
    pub tracks: Vec<DzTrack>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DzArtistPage {
    pub artist: DzArtist,
    pub top_tracks: Vec<DzTrack>,
    pub albums: Vec<DzAlbumDetail>,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeezerUser {
    pub id: u64,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub picture: Option<String>,
    pub can_stream_lossless: bool,
    pub can_stream_hq: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
}

// ---------------------------------------------------------------------------
// Lenient field readers
// ---------------------------------------------------------------------------

/// A positive integer from a number or a numeric string. Deezer user uploads
/// carry negative ids, which SONE cannot stream, so those read as `None`.
pub fn num(v: &Value, key: &str) -> Option<u64> {
    match v.get(key)? {
        Value::Number(n) => n
            .as_u64()
            .or_else(|| n.as_f64().filter(|f| *f >= 0.0).map(|f| f as u64)),
        Value::String(s) => s.trim().parse::<u64>().ok(),
        _ => None,
    }
}

pub fn float(v: &Value, key: &str) -> Option<f64> {
    match v.get(key)? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

pub fn text(v: &Value, key: &str) -> Option<String> {
    match v.get(key)? {
        Value::String(s) if !s.trim().is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

pub fn flag(v: &Value, key: &str) -> Option<bool> {
    match v.get(key)? {
        Value::Bool(b) => Some(*b),
        Value::Number(n) => n.as_i64().map(|i| i != 0),
        Value::String(s) => match s.as_str() {
            "1" | "true" => Some(true),
            "0" | "false" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn u32_of(v: &Value, key: &str) -> Option<u32> {
    num(v, key).and_then(|n| u32::try_from(n).ok())
}

/// Full image URL from a gw-light picture hash (`kind`: cover, artist, user,
/// playlist).
pub fn image_url(kind: &str, md5: Option<String>) -> Option<String> {
    let md5 = md5?;
    if md5.starts_with("http") {
        return Some(md5);
    }
    Some(format!(
        "{IMAGE_HOST}/{kind}/{md5}/1000x1000-000000-80-0-0.jpg"
    ))
}

/// Largest picture the public API offers.
fn public_picture(v: &Value, base: &str) -> Option<String> {
    ["_xl", "_big", "_medium", ""]
        .iter()
        .find_map(|s| text(v, &format!("{base}{s}")))
}

/// Unix seconds → `YYYY-MM-DDTHH:MM:SS.000+0000`, the form TIDAL uses for
/// `dateAdded` (no chrono in the dependency tree for one conversion).
pub fn unix_to_iso(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Howard Hinnant's civil-from-days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.000+0000",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

fn record_type(raw: Option<String>) -> Option<String> {
    raw.map(|t| match t.to_ascii_lowercase().as_str() {
        "ep" | "1" => "EP".to_string(),
        "single" | "3" => "SINGLE".to_string(),
        _ => "ALBUM".to_string(),
    })
}

// ---------------------------------------------------------------------------
// Public API (api.deezer.com)
// ---------------------------------------------------------------------------

pub fn public_artist(v: &Value) -> Option<DzArtist> {
    Some(DzArtist {
        id: num(v, "id")?,
        name: text(v, "name")?,
        picture: public_picture(v, "picture"),
        artist_type: v.get("role").and_then(|r| r.as_str()).map(|r| {
            if r.eq_ignore_ascii_case("main") {
                "MAIN".to_string()
            } else {
                "FEATURED".to_string()
            }
        }),
        provider: PROVIDER,
    })
}

fn public_artists(v: &Value) -> (Option<DzArtist>, Option<Vec<DzArtist>>) {
    let main = v.get("artist").and_then(public_artist);
    let contributors: Vec<DzArtist> = v
        .get("contributors")
        .and_then(|c| c.as_array())
        .map(|a| a.iter().filter_map(public_artist).collect())
        .unwrap_or_default();
    let artists = if contributors.is_empty() {
        main.clone().map(|a| vec![a])
    } else {
        Some(contributors)
    };
    (main, artists)
}

fn public_album_ref(v: &Value) -> Option<DzAlbumRef> {
    Some(DzAlbumRef {
        id: num(v, "id")?,
        title: text(v, "title")?,
        cover: public_picture(v, "cover"),
        release_date: text(v, "release_date"),
    })
}

/// A track from api.deezer.com. `album` is supplied by callers whose payload
/// omits it (album track listings).
pub fn public_track(v: &Value, album: Option<&DzAlbumRef>) -> Option<DzTrack> {
    let id = num(v, "id")?;
    let (artist, artists) = public_artists(v);
    let mut album_ref = v
        .get("album")
        .and_then(public_album_ref)
        .or_else(|| album.cloned());
    if let (Some(a), Some(rd)) = (album_ref.as_mut(), text(v, "release_date")) {
        a.release_date.get_or_insert(rd);
    }
    let readable = flag(v, "readable");
    Some(DzTrack {
        id,
        title: text(v, "title_short").or_else(|| text(v, "title"))?,
        duration: u32_of(v, "duration").unwrap_or(0),
        version: text(v, "title_version"),
        artist,
        artists,
        album: album_ref,
        audio_quality: None,
        track_number: u32_of(v, "track_position"),
        volume_number: u32_of(v, "disk_number"),
        date_added: num(v, "time_add").map(unix_to_iso),
        isrc: text(v, "isrc"),
        explicit: flag(v, "explicit_lyrics"),
        replay_gain: float(v, "gain"),
        peak: None,
        url: text(v, "link"),
        stream_ready: readable,
        allow_streaming: readable,
        provider: PROVIDER,
    })
}

pub fn public_tracks(list: &Value, album: Option<&DzAlbumRef>) -> Vec<DzTrack> {
    list.as_array()
        .map(|a| a.iter().filter_map(|t| public_track(t, album)).collect())
        .unwrap_or_default()
}

pub fn public_album(v: &Value) -> Option<DzAlbumDetail> {
    let (artist, artists) = public_artists(v);
    Some(DzAlbumDetail {
        id: num(v, "id")?,
        title: text(v, "title")?,
        version: None,
        cover: public_picture(v, "cover"),
        artist,
        artists,
        number_of_tracks: u32_of(v, "nb_tracks"),
        number_of_videos: Some(0),
        number_of_volumes: None,
        duration: u32_of(v, "duration"),
        release_date: text(v, "release_date"),
        upc: text(v, "upc"),
        album_type: record_type(text(v, "record_type")),
        copyright: None,
        explicit: flag(v, "explicit_lyrics"),
        url: text(v, "link"),
        audio_quality: None,
        provider: PROVIDER,
    })
}

pub fn album_ref_of(a: &DzAlbumDetail) -> DzAlbumRef {
    DzAlbumRef {
        id: a.id,
        title: a.title.clone(),
        cover: a.cover.clone(),
        release_date: a.release_date.clone(),
    }
}

pub fn public_playlist(v: &Value) -> Option<DzPlaylist> {
    let id = num(v, "id")?;
    let picture = public_picture(v, "picture");
    // Full playlist objects carry `creator`; search results carry `user`.
    let creator = v.get("creator").or_else(|| v.get("user")).and_then(|c| {
        Some(DzPlaylistCreator {
            id: num(c, "id")?,
            name: text(c, "name"),
        })
    });
    Some(DzPlaylist {
        uuid: id.to_string(),
        title: text(v, "title")?,
        description: text(v, "description"),
        image: picture.clone(),
        square_image: picture,
        number_of_tracks: u32_of(v, "nb_tracks"),
        number_of_videos: Some(0),
        creator,
        playlist_type: Some("USER".to_string()),
        duration: u32_of(v, "duration"),
        last_updated: num(v, "time_mod").map(unix_to_iso),
        access_type: flag(v, "public").map(|p| {
            if p {
                "PUBLIC".to_string()
            } else {
                "UNLISTED".to_string()
            }
        }),
        provider: PROVIDER,
    })
}

// ---------------------------------------------------------------------------
// gw-light (www.deezer.com/ajax/gw-light.php)
// ---------------------------------------------------------------------------

fn gw_artist(v: &Value) -> Option<DzArtist> {
    Some(DzArtist {
        id: num(v, "ART_ID")?,
        name: text(v, "ART_NAME")?,
        picture: image_url("artist", text(v, "ART_PICTURE")),
        artist_type: num(v, "ARTISTS_SONGS_ORDER")
            .map(|o| if o == 0 { "MAIN" } else { "FEATURED" }.to_string()),
        provider: PROVIDER,
    })
}

/// A gw-light song object (`song.getData`, `song.getListData`, ...).
pub fn gw_track(v: &Value) -> Option<DzTrack> {
    let id = num(v, "SNG_ID")?;
    let artists: Vec<DzArtist> = v
        .get("ARTISTS")
        .and_then(|a| a.as_array())
        .map(|a| a.iter().filter_map(gw_artist).collect())
        .unwrap_or_default();
    // ARTISTS entries carry the MAIN/FEATURED order; the flat ART_* fields do not.
    let artist = artists.first().cloned().or_else(|| gw_artist(v));
    let artists = if artists.is_empty() {
        artist.clone().map(|a| vec![a])
    } else {
        Some(artists)
    };
    let album = num(v, "ALB_ID").map(|aid| DzAlbumRef {
        id: aid,
        title: text(v, "ALB_TITLE").unwrap_or_default(),
        cover: image_url("cover", text(v, "ALB_PICTURE")),
        release_date: text(v, "PHYSICAL_RELEASE_DATE").or_else(|| text(v, "DIGITAL_RELEASE_DATE")),
    });
    let explicit = flag(v, "EXPLICIT_LYRICS").or_else(|| {
        v.get("EXPLICIT_TRACK_CONTENT")
            .and_then(|e| num(e, "EXPLICIT_LYRICS_STATUS"))
            .map(|s| s == 1 || s == 4)
    });
    let has_flac = num(v, "FILESIZE_FLAC").is_some_and(|s| s > 0);
    Some(DzTrack {
        id,
        title: text(v, "SNG_TITLE")?,
        duration: u32_of(v, "DURATION").unwrap_or(0),
        version: text(v, "VERSION"),
        artist,
        artists,
        album,
        audio_quality: v
            .get("FILESIZE_FLAC")
            .is_some()
            .then(|| if has_flac { "LOSSLESS" } else { "HIGH" }.to_string()),
        track_number: u32_of(v, "TRACK_NUMBER"),
        volume_number: u32_of(v, "DISK_NUMBER"),
        date_added: num(v, "DATE_ADD").map(unix_to_iso),
        isrc: text(v, "ISRC"),
        explicit,
        replay_gain: float(v, "GAIN"),
        peak: None,
        url: Some(format!("https://www.deezer.com/track/{id}")),
        stream_ready: Some(v.get("TRACK_TOKEN").is_some() || v.get("MD5_ORIGIN").is_some()),
        allow_streaming: Some(true),
        provider: PROVIDER,
    })
}

pub fn gw_tracks(list: &Value) -> Vec<DzTrack> {
    list.as_array()
        .map(|a| a.iter().filter_map(gw_track).collect())
        .unwrap_or_default()
}

pub fn gw_album(v: &Value) -> Option<DzAlbumDetail> {
    let artist = gw_artist(v);
    Some(DzAlbumDetail {
        id: num(v, "ALB_ID")?,
        title: text(v, "ALB_TITLE")?,
        version: text(v, "VERSION"),
        cover: image_url("cover", text(v, "ALB_PICTURE")),
        artists: artist.clone().map(|a| vec![a]),
        artist,
        number_of_tracks: u32_of(v, "NUMBER_TRACK").or_else(|| u32_of(v, "NB_SONG")),
        number_of_videos: Some(0),
        number_of_volumes: u32_of(v, "NUMBER_DISK"),
        duration: u32_of(v, "DURATION"),
        release_date: text(v, "PHYSICAL_RELEASE_DATE")
            .or_else(|| text(v, "DIGITAL_RELEASE_DATE"))
            .or_else(|| text(v, "ORIGINAL_RELEASE_DATE")),
        upc: text(v, "UPC"),
        album_type: record_type(text(v, "TYPE")),
        copyright: text(v, "COPYRIGHT"),
        explicit: v
            .get("EXPLICIT_ALBUM_CONTENT")
            .and_then(|e| num(e, "EXPLICIT_LYRICS_STATUS"))
            .map(|s| s == 1 || s == 4),
        url: None,
        audio_quality: None,
        provider: PROVIDER,
    })
    .map(|mut a| {
        a.url = Some(format!("https://www.deezer.com/album/{}", a.id));
        a
    })
}

pub fn gw_playlist(v: &Value) -> Option<DzPlaylist> {
    let id = num(v, "PLAYLIST_ID")?;
    let kind = text(v, "PICTURE_TYPE").unwrap_or_else(|| "playlist".to_string());
    let picture = image_url(&kind, text(v, "PLAYLIST_PICTURE"));
    let creator = num(v, "PARENT_USER_ID").map(|uid| DzPlaylistCreator {
        id: uid,
        name: text(v, "PARENT_USERNAME"),
    });
    Some(DzPlaylist {
        uuid: id.to_string(),
        title: text(v, "TITLE")?,
        description: text(v, "DESCRIPTION"),
        image: picture.clone(),
        square_image: picture,
        number_of_tracks: u32_of(v, "NB_SONG"),
        number_of_videos: Some(0),
        creator,
        playlist_type: Some("USER".to_string()),
        duration: u32_of(v, "DURATION"),
        last_updated: text(v, "DATE_MOD"),
        // STATUS: 0 public, 1 private, 2 collaborative.
        access_type: num(v, "STATUS").map(|s| {
            if s == 1 {
                "UNLISTED".to_string()
            } else {
                "PUBLIC".to_string()
            }
        }),
        provider: PROVIDER,
    })
}

/// `deezer.getUserData` → user + license token. `None` for an anonymous
/// session (USER_ID 0), which is what an invalid or expired ARL produces.
pub fn gw_user(results: &Value) -> Option<(DeezerUser, String)> {
    let user = results.get("USER")?;
    let id = num(user, "USER_ID").filter(|&id| id != 0)?;
    let options = user.get("OPTIONS").cloned().unwrap_or(Value::Null);
    let opt = |k: &str| flag(&options, k).unwrap_or(false);
    let license = text(&options, "license_token").unwrap_or_default();
    Some((
        DeezerUser {
            id,
            name: text(user, "BLOG_NAME")
                .or_else(|| text(user, "FIRSTNAME"))
                .unwrap_or_else(|| format!("Deezer user {id}")),
            picture: image_url("user", text(user, "USER_PICTURE")),
            can_stream_lossless: opt("web_lossless") || opt("mobile_lossless") || opt("lossless"),
            can_stream_hq: opt("web_hq") || opt("mobile_hq"),
            country: text(results, "COUNTRY").or_else(|| text(&options, "license_country")),
        },
        license,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Shaped after api.deezer.com/search/track (the sandbox network blocks
    // api.deezer.com, so this is written from the documented public schema).
    fn public_search_track() -> Value {
        json!({
            "id": 3135556,
            "readable": true,
            "title": "Harder, Better, Faster, Stronger",
            "title_short": "Harder, Better, Faster, Stronger",
            "title_version": "",
            "link": "https://www.deezer.com/track/3135556",
            "duration": 224,
            "rank": 949594,
            "explicit_lyrics": false,
            "explicit_content_lyrics": 0,
            "preview": "https://cdnt-preview.dzcdn.net/api/1/1/x.mp3",
            "md5_image": "2e018122cb56986277102d2041a592c8",
            "artist": {
                "id": 27, "name": "Daft Punk",
                "link": "https://www.deezer.com/artist/27",
                "picture": "https://api.deezer.com/artist/27/image",
                "picture_medium": "https://cdn-images.dzcdn.net/images/artist/f2bc/250x250.jpg",
                "picture_xl": "https://cdn-images.dzcdn.net/images/artist/f2bc/1000x1000.jpg",
                "type": "artist"
            },
            "album": {
                "id": 302127, "title": "Discovery",
                "cover": "https://api.deezer.com/album/302127/image",
                "cover_medium": "https://cdn-images.dzcdn.net/images/cover/2e01/250x250.jpg",
                "cover_xl": "https://cdn-images.dzcdn.net/images/cover/2e01/1000x1000.jpg",
                "type": "album"
            },
            "type": "track"
        })
    }

    #[test]
    fn deezer_public_track_maps_to_sone_track_shape() {
        let t = public_track(&public_search_track(), None).unwrap();
        let j = serde_json::to_value(&t).unwrap();
        assert_eq!(j["id"], 3135556);
        assert_eq!(j["title"], "Harder, Better, Faster, Stronger");
        assert_eq!(
            j["version"],
            Value::Null,
            "empty title_version is no version"
        );
        assert_eq!(j["duration"], 224);
        assert_eq!(j["provider"], "deezer");
        assert_eq!(j["artist"]["name"], "Daft Punk");
        assert_eq!(j["artist"]["provider"], "deezer");
        assert_eq!(
            j["artist"]["picture"],
            "https://cdn-images.dzcdn.net/images/artist/f2bc/1000x1000.jpg"
        );
        assert_eq!(j["artists"][0]["id"], 27);
        assert_eq!(j["album"]["id"], 302127);
        assert_eq!(
            j["album"]["cover"],
            "https://cdn-images.dzcdn.net/images/cover/2e01/1000x1000.jpg"
        );
        assert_eq!(j["explicit"], false);
        assert_eq!(j["allowStreaming"], true);
        // camelCase keys, like TidalTrack.
        assert!(j.get("trackNumber").is_some() && j.get("track_number").is_none());
    }

    #[test]
    fn deezer_album_tracks_get_parent_album_injected() {
        let album = public_album(&json!({
            "id": 302127, "title": "Discovery", "upc": "724384960650",
            "cover_xl": "https://cdn-images.dzcdn.net/images/cover/2e01/1000x1000.jpg",
            "nb_tracks": 14, "duration": 3660, "release_date": "2001-03-07",
            "record_type": "album", "explicit_lyrics": false,
            "link": "https://www.deezer.com/album/302127",
            "contributors": [{"id": 27, "name": "Daft Punk", "role": "Main",
                              "picture_xl": "https://x/1000.jpg"}],
            "artist": {"id": 27, "name": "Daft Punk", "picture_xl": "https://x/1000.jpg"}
        }))
        .unwrap();
        let j = serde_json::to_value(&album).unwrap();
        assert_eq!(j["type"], "ALBUM");
        assert_eq!(j["numberOfTracks"], 14);
        assert_eq!(j["releaseDate"], "2001-03-07");
        assert_eq!(j["artists"][0]["type"], "MAIN");
        assert_eq!(j["provider"], "deezer");

        let r = album_ref_of(&album);
        let tracks = public_tracks(
            &json!([{"id": 3135553, "readable": true, "title": "One More Time",
                     "title_short": "One More Time", "title_version": "(Radio Edit)",
                     "duration": 320, "track_position": 1, "disk_number": 1,
                     "isrc": "GBDUW0000053", "explicit_lyrics": false, "gain": -12.4,
                     "artist": {"id": 27, "name": "Daft Punk"}}]),
            Some(&r),
        );
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].album.as_ref().unwrap().id, 302127);
        assert_eq!(tracks[0].version.as_deref(), Some("(Radio Edit)"));
        assert_eq!(tracks[0].track_number, Some(1));
        assert_eq!(tracks[0].replay_gain, Some(-12.4));
    }

    #[test]
    fn deezer_public_playlist_uses_string_uuid() {
        let p = public_playlist(&json!({
            "id": 1479458365, "title": "Daft Punk Essentials", "public": true,
            "nb_tracks": 50, "picture_xl": "https://x/pl.jpg",
            "user": {"id": 2529, "name": "Deezer Editor"}
        }))
        .unwrap();
        let j = serde_json::to_value(&p).unwrap();
        assert_eq!(j["uuid"], "1479458365");
        assert_eq!(j["numberOfTracks"], 50);
        assert_eq!(j["creator"]["id"], 2529);
        assert_eq!(j["accessType"], "PUBLIC");
        assert_eq!(j["squareImage"], "https://x/pl.jpg");
        assert_eq!(j["provider"], "deezer");
    }

    // Shaped after a gw-light `song.getListData` entry.
    fn gw_song() -> Value {
        json!({
            "SNG_ID": "3135556",
            "SNG_TITLE": "Harder, Better, Faster, Stronger",
            "VERSION": "",
            "ART_ID": "27", "ART_NAME": "Daft Punk",
            "ARTISTS": [{"ART_ID": "27", "ART_NAME": "Daft Punk",
                         "ART_PICTURE": "f2bc007e9133c946ac3c3907ddc5d2ea",
                         "ARTISTS_SONGS_ORDER": "0"}],
            "ALB_ID": "302127", "ALB_TITLE": "Discovery",
            "ALB_PICTURE": "2e018122cb56986277102d2041a592c8",
            "DURATION": "224", "TRACK_NUMBER": "4", "DISK_NUMBER": "1",
            "ISRC": "GBDUW0000059", "GAIN": "-11.3",
            "EXPLICIT_LYRICS": "0",
            "PHYSICAL_RELEASE_DATE": "2001-03-07",
            "MD5_ORIGIN": "a1b2c3", "FILESIZE_FLAC": "25000000",
            "FILESIZE_MP3_320": "9000000", "FILESIZE_MP3_128": "3600000",
            "TRACK_TOKEN": "tok", "TRACK_TOKEN_EXPIRE": 1760000000,
            "DATE_ADD": 1700000000,
            "__TYPE__": "song"
        })
    }

    #[test]
    fn deezer_gw_song_maps_to_sone_track_shape() {
        let t = gw_track(&gw_song()).unwrap();
        assert_eq!(t.id, 3135556);
        assert_eq!(t.duration, 224);
        assert_eq!(t.version, None);
        assert_eq!(t.track_number, Some(4));
        assert_eq!(t.replay_gain, Some(-11.3));
        assert_eq!(t.audio_quality.as_deref(), Some("LOSSLESS"));
        assert_eq!(t.explicit, Some(false));
        assert_eq!(
            t.date_added.as_deref(),
            Some("2023-11-14T22:13:20.000+0000")
        );
        assert_eq!(
            t.album.as_ref().unwrap().cover.as_deref(),
            Some("https://cdn-images.dzcdn.net/images/cover/2e018122cb56986277102d2041a592c8/1000x1000-000000-80-0-0.jpg")
        );
        assert_eq!(
            t.artist.as_ref().unwrap().artist_type.as_deref(),
            Some("MAIN")
        );
        let j = serde_json::to_value(&t).unwrap();
        assert_eq!(j["provider"], "deezer");
        assert_eq!(j["album"]["releaseDate"], "2001-03-07");
    }

    #[test]
    fn deezer_gw_negative_ids_are_skipped() {
        let mut s = gw_song();
        s["SNG_ID"] = json!("-123456");
        assert!(gw_track(&s).is_none());
        assert_eq!(gw_tracks(&json!([s, gw_song()])).len(), 1);
    }

    #[test]
    fn deezer_gw_playlist_and_album_map() {
        let p = gw_playlist(&json!({
            "PLAYLIST_ID": "908622995", "TITLE": "Loved", "NB_SONG": 12,
            "PLAYLIST_PICTURE": "abc", "PICTURE_TYPE": "cover", "STATUS": 1,
            "PARENT_USER_ID": "5", "PARENT_USERNAME": "etienne",
            "DATE_MOD": "2024-01-02 03:04:05"
        }))
        .unwrap();
        assert_eq!(p.uuid, "908622995");
        assert_eq!(p.access_type.as_deref(), Some("UNLISTED"));
        assert_eq!(
            p.image.as_deref(),
            Some("https://cdn-images.dzcdn.net/images/cover/abc/1000x1000-000000-80-0-0.jpg")
        );
        let a = gw_album(&json!({
            "ALB_ID": "302127", "ALB_TITLE": "Discovery", "ALB_PICTURE": "2e01",
            "ART_ID": "27", "ART_NAME": "Daft Punk", "NUMBER_TRACK": "14",
            "PHYSICAL_RELEASE_DATE": "2001-03-07", "TYPE": "1"
        }))
        .unwrap();
        assert_eq!(a.number_of_tracks, Some(14));
        assert_eq!(a.album_type.as_deref(), Some("EP"));
        assert_eq!(
            a.url.as_deref(),
            Some("https://www.deezer.com/album/302127")
        );
    }

    #[test]
    fn deezer_gw_user_data_maps_to_deezer_user() {
        let results = json!({
            "checkForm": "api-token",
            "COUNTRY": "FR",
            "USER": {
                "USER_ID": 123456789, "BLOG_NAME": "Etienne",
                "USER_PICTURE": "pic",
                "OPTIONS": {"license_token": "lic", "web_hq": true,
                            "web_lossless": true, "mobile_lossless": false}
            }
        });
        let (u, lic) = gw_user(&results).unwrap();
        assert_eq!(lic, "lic");
        let j = serde_json::to_value(&u).unwrap();
        assert_eq!(j["id"], 123456789);
        assert_eq!(j["name"], "Etienne");
        assert_eq!(j["canStreamLossless"], true);
        assert_eq!(j["canStreamHq"], true);
        assert_eq!(j["country"], "FR");

        let anon = json!({"USER": {"USER_ID": 0, "OPTIONS": {}}});
        assert!(gw_user(&anon).is_none(), "invalid ARL → anonymous session");
    }

    #[test]
    fn deezer_unix_to_iso() {
        assert_eq!(unix_to_iso(0), "1970-01-01T00:00:00.000+0000");
        assert_eq!(unix_to_iso(951_782_400), "2000-02-29T00:00:00.000+0000");
    }
}

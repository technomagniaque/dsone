//! Wire level of the three Deezer endpoints SONE talks to:
//!
//! * `www.deezer.com/ajax/gw-light.php` — the web player's private gateway
//!   (session, favorites, user library, track tokens). Authenticated by the
//!   `arl` cookie plus the `sid` cookie and `checkForm` token it hands back.
//! * `api.deezer.com` — the public catalogue API (search, album, artist,
//!   playlist). No auth.
//! * `media.deezer.com/v1/get_url` — turns a track token + license token
//!   into a CDN URL for the encrypted file.
//!
//! Every request goes out through the shared `ProxiedHttp` cell, exactly like
//! `TidalClient`: the client is taken from the cell per request (so a proxy
//! change applies to the next request) and the outcome is reported back with
//! `observe_at` so the proxy status screen sees Deezer traffic too.
//!
//! Cookies: the shared client is built without a cookie store (reqwest's
//! `cookies` feature is not enabled, and a store on the shared client would
//! leak Deezer cookies into every other consumer). Rather than building a
//! second client — which would need to replicate the proxy plan and could
//! drift from it — the `arl`/`sid` cookies are carried by hand in a `Cookie`
//! header, and `Set-Cookie` answers are folded back into the session by
//! `DeezerClient`. That keeps exactly one egress path.

use reqwest::header::{HeaderMap, COOKIE, SET_COOKIE, USER_AGENT};
use serde_json::{json, Value};

use crate::proxy_http::ProxiedHttp;
use crate::SoneError;

pub const GW_URL: &str = "https://www.deezer.com/ajax/gw-light.php";
pub const PUBLIC_URL: &str = "https://api.deezer.com";
pub const MEDIA_URL: &str = "https://media.deezer.com/v1/get_url";
/// gw-light rejects obviously non-browser agents.
const BROWSER_UA: &str =
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

/// Thin wrapper over the shared proxied cell.
#[derive(Clone)]
pub struct Http(pub ProxiedHttp);

impl Http {
    /// Send one request built on the current shared client. Reports the
    /// outcome to the cell (see `ProxiedHttp::observe_at`).
    pub async fn send(
        &self,
        build: impl FnOnce(&reqwest::Client) -> reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, SoneError> {
        let (generation, client) = self.0.client_at();
        let client = client.map_err(|e| SoneError::ProxyBlocked { reason: e.cause })?;
        let req = build(&client).header(USER_AGENT, BROWSER_UA);
        Ok(self.0.observe_at(generation, req.send().await)?)
    }
}

// ---------------------------------------------------------------------------
// gw-light
// ---------------------------------------------------------------------------

/// What a gw-light `error` field means for the caller.
#[derive(Debug, PartialEq)]
pub enum GwError {
    /// The `checkForm` api_token is missing or stale: refresh and retry once.
    InvalidToken,
    Other(String),
}

/// `error` is `[]` or `{}` on success, an object of `CODE: message` otherwise.
pub fn gw_error(json: &Value) -> Option<GwError> {
    let err = json.get("error")?;
    let empty = match err {
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        Value::Null => true,
        Value::String(s) => s.is_empty(),
        _ => false,
    };
    if empty {
        return None;
    }
    let s = err.to_string();
    if s.contains("VALID_TOKEN_REQUIRED")
        || s.to_ascii_lowercase().contains("invalid api token")
        || s.contains("NEED_API_AUTH_REQUIRED")
    {
        return Some(GwError::InvalidToken);
    }
    Some(GwError::Other(s))
}

/// `name=value` pairs from `Set-Cookie` headers (attributes dropped).
pub fn set_cookies(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .get_all(SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .filter_map(|v| {
            let pair = v.split(';').next()?.trim();
            let (name, value) = pair.split_once('=')?;
            let name = name.trim();
            (!name.is_empty()).then(|| (name.to_string(), value.trim().to_string()))
        })
        .collect()
}

/// One gw-light call. Returns the whole JSON answer (callers inspect
/// `error` and read `results`) and any cookies the gateway set.
pub async fn gw_call(
    http: &Http,
    method: &str,
    api_token: &str,
    cookie_header: &str,
    body: &Value,
) -> Result<(Value, Vec<(String, String)>), SoneError> {
    let resp = http
        .send(|c| {
            c.post(GW_URL)
                .query(&[
                    ("method", method),
                    ("input", "3"),
                    ("api_version", "1.0"),
                    ("api_token", api_token),
                ])
                .header(COOKIE, cookie_header)
                .json(body)
        })
        .await?;
    let status = resp.status();
    let cookies = set_cookies(resp.headers());
    let text = resp.text().await?;
    if !status.is_success() {
        // Never echo the body: gw answers can carry account data.
        return Err(SoneError::Api {
            status: status.as_u16(),
            body: format!("deezer gateway {method} failed"),
        });
    }
    let json: Value = serde_json::from_str(&text)
        .map_err(|e| SoneError::Parse(format!("deezer gateway {method}: {e}")))?;
    Ok((json, cookies))
}

// ---------------------------------------------------------------------------
// api.deezer.com
// ---------------------------------------------------------------------------

/// Map the public API's in-band error (`200 {"error":{...}}`) to an HTTP-like
/// status so `SoneError` classification (404 terminal, 429 rate limit) works.
pub fn public_error(json: &Value) -> Option<SoneError> {
    let err = json.get("error")?;
    if !err.is_object() {
        return None;
    }
    let code = err.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
    let status = match code {
        4 => 429,   // Quota
        800 => 404, // DataException "no data"
        200 | 300 => 401,
        _ => 400,
    };
    Some(SoneError::Api {
        status,
        body: err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("deezer api error")
            .to_string(),
    })
}

/// GET `api.deezer.com/<path>` with query params.
pub async fn public_get(
    http: &Http,
    path: &str,
    query: &[(&str, String)],
) -> Result<Value, SoneError> {
    let url = format!("{PUBLIC_URL}/{}", path.trim_start_matches('/'));
    let resp = http.send(|c| c.get(&url).query(query)).await?;
    let status = resp.status();
    if !status.is_success() {
        return Err(SoneError::Api {
            status: status.as_u16(),
            body: format!("deezer api {path}"),
        });
    }
    let json: Value = resp.json().await?;
    if let Some(e) = public_error(&json) {
        return Err(e);
    }
    Ok(json)
}

/// GET a paginated list (`{"data": [...], "next": "..."}`), following `next`
/// until `max` items or the end.
pub async fn public_get_all(http: &Http, path: &str, max: usize) -> Result<Vec<Value>, SoneError> {
    const PAGE: usize = 100;
    let mut out: Vec<Value> = Vec::new();
    loop {
        let page = public_get(
            http,
            path,
            &[
                ("index", out.len().to_string()),
                ("limit", PAGE.to_string()),
            ],
        )
        .await?;
        let items = page
            .get("data")
            .and_then(|d| d.as_array())
            .cloned()
            .unwrap_or_default();
        let got = items.len();
        out.extend(items);
        if got == 0 || out.len() >= max || page.get("next").is_none() {
            break;
        }
    }
    out.truncate(max);
    Ok(out)
}

// ---------------------------------------------------------------------------
// media.deezer.com
// ---------------------------------------------------------------------------

/// Outcome of `get_url` for one track token.
#[derive(Debug, PartialEq)]
pub enum MediaAnswer {
    /// The format Deezer chose (first of the requested ones it can serve).
    Url { format: String, url: String },
    /// No requested format is available for this token.
    Unavailable(String),
    /// The track token has expired; fetch fresh song data and retry.
    TokenExpired,
}

pub fn get_url_body(license_token: &str, track_token: &str, formats: &[&str]) -> Value {
    json!({
        "license_token": license_token,
        "media": [{
            "type": "FULL",
            "formats": formats
                .iter()
                .map(|f| json!({"cipher": "BF_CBC_STRIPE", "format": f}))
                .collect::<Vec<_>>(),
        }],
        "track_tokens": [track_token],
    })
}

fn media_errors(v: &Value) -> Option<Vec<(i64, String)>> {
    let errs = v.get("errors")?.as_array()?;
    if errs.is_empty() {
        return None;
    }
    Some(
        errs.iter()
            .map(|e| {
                (
                    e.get("code").and_then(|c| c.as_i64()).unwrap_or(0),
                    e.get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("")
                        .to_string(),
                )
            })
            .collect(),
    )
}

pub fn parse_get_url(json: &Value) -> MediaAnswer {
    let entry = json
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|d| d.first());
    let errors = entry
        .and_then(media_errors)
        .or_else(|| media_errors(json))
        .unwrap_or_default();
    // 2001: "Track token has expired".
    if errors.iter().any(|(c, _)| *c == 2001) {
        return MediaAnswer::TokenExpired;
    }
    let media = entry
        .and_then(|e| e.get("media"))
        .and_then(|m| m.as_array())
        .and_then(|m| m.first());
    if let Some(m) = media {
        let format = m.get("format").and_then(|f| f.as_str());
        let url = m
            .get("sources")
            .and_then(|s| s.as_array())
            .and_then(|s| s.first())
            .and_then(|s| s.get("url"))
            .and_then(|u| u.as_str());
        if let (Some(format), Some(url)) = (format, url) {
            return MediaAnswer::Url {
                format: format.to_string(),
                url: url.to_string(),
            };
        }
    }
    let why = errors
        .iter()
        .map(|(c, m)| format!("{c}: {m}"))
        .collect::<Vec<_>>()
        .join("; ");
    MediaAnswer::Unavailable(if why.is_empty() {
        "no media returned".to_string()
    } else {
        why
    })
}

pub async fn get_url(
    http: &Http,
    license_token: &str,
    track_token: &str,
    formats: &[&str],
) -> Result<MediaAnswer, SoneError> {
    let body = get_url_body(license_token, track_token, formats);
    let resp = http.send(|c| c.post(MEDIA_URL).json(&body)).await?;
    let status = resp.status();
    let text = resp.text().await?;
    let json: Value = match serde_json::from_str(&text) {
        Ok(j) => j,
        Err(_) if !status.is_success() => {
            return Err(SoneError::Api {
                status: status.as_u16(),
                body: "deezer media get_url failed".into(),
            })
        }
        Err(e) => return Err(SoneError::Parse(format!("deezer get_url: {e}"))),
    };
    Ok(parse_get_url(&json))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deezer_gw_error_classification() {
        assert_eq!(gw_error(&json!({"error": [], "results": {}})), None);
        assert_eq!(gw_error(&json!({"error": {}, "results": {}})), None);
        assert_eq!(
            gw_error(&json!({"error": {"VALID_TOKEN_REQUIRED": "Invalid CSRF token"}})),
            Some(GwError::InvalidToken)
        );
        assert_eq!(
            gw_error(&json!({"error": {"GATEWAY_ERROR": "invalid api token"}})),
            Some(GwError::InvalidToken)
        );
        assert!(matches!(
            gw_error(&json!({"error": {"DATA_ERROR": "song not found"}})),
            Some(GwError::Other(_))
        ));
    }

    #[test]
    fn deezer_set_cookie_parsing() {
        let mut h = HeaderMap::new();
        h.append(
            SET_COOKIE,
            "sid=fr1234abcd; expires=Thu, 01-Jan-2099 00:00:00 GMT; path=/; domain=.deezer.com; secure; HttpOnly"
                .parse()
                .unwrap(),
        );
        h.append(
            SET_COOKIE,
            "dzr_uniq_id=dzr_uniq_id_fr; path=/".parse().unwrap(),
        );
        assert_eq!(
            set_cookies(&h),
            vec![
                ("sid".to_string(), "fr1234abcd".to_string()),
                ("dzr_uniq_id".to_string(), "dzr_uniq_id_fr".to_string())
            ]
        );
    }

    #[test]
    fn deezer_public_error_maps_status() {
        let e = public_error(
            &json!({"error": {"type": "DataException", "message": "no data", "code": 800}}),
        )
        .unwrap();
        assert!(e.is_terminal_unplayable());
        let q = public_error(
            &json!({"error": {"type": "Exception", "message": "Quota limit exceeded", "code": 4}}),
        )
        .unwrap();
        assert!(q.is_rate_limited());
        assert!(public_error(&json!({"data": []})).is_none());
    }

    #[test]
    fn deezer_get_url_body_and_parse() {
        let body = get_url_body("lic", "tok", &["FLAC", "MP3_320", "MP3_128"]);
        assert_eq!(body["media"][0]["type"], "FULL");
        assert_eq!(body["media"][0]["formats"][0]["cipher"], "BF_CBC_STRIPE");
        assert_eq!(body["media"][0]["formats"][2]["format"], "MP3_128");
        assert_eq!(body["track_tokens"][0], "tok");

        let ok = json!({"data": [{"media": [{
            "media_type": "FULL", "cipher": {"type": "BF_CBC_STRIPE"},
            "format": "MP3_320",
            "sources": [{"url": "https://e-cdns-proxy-a.dzcdn.net/mobile/1/abc", "provider": "ak"}],
            "nbf": 1, "exp": 2
        }]}]});
        assert_eq!(
            parse_get_url(&ok),
            MediaAnswer::Url {
                format: "MP3_320".into(),
                url: "https://e-cdns-proxy-a.dzcdn.net/mobile/1/abc".into()
            }
        );

        let expired =
            json!({"data": [{"errors": [{"code": 2001, "message": "Track token has expired"}]}]});
        assert_eq!(parse_get_url(&expired), MediaAnswer::TokenExpired);

        let rights = json!({"data": [{"errors": [{"code": 2002, "message": "Track token has no sufficient rights on requested media"}]}]});
        assert!(
            matches!(parse_get_url(&rights), MediaAnswer::Unavailable(m) if m.contains("2002"))
        );
        assert!(matches!(
            parse_get_url(&json!({"data": [{"media": []}]})),
            MediaAnswer::Unavailable(_)
        ));
    }
}

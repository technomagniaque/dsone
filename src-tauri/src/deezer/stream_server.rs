//! Loopback HTTP server that serves decrypted Deezer audio to GStreamer.
//!
//! `play_deezer_track` hands the audio pipeline a URL of the form
//! `http://127.0.0.1:<port>/deezer/<token>/<trackId>.<flac|mp3>`. A request
//! resolves the CDN URL (cached per track by `DeezerClient`), fetches the
//! encrypted file through the shared proxied client, decrypts the Blowfish
//! stripes while streaming and answers with the exact byte range asked for.
//!
//! Range handling: decryption works on whole 2048-byte chunks whose parity
//! (`index % 3`) depends on the absolute offset, so the upstream request
//! starts at the client's offset aligned *down* to a chunk boundary, the
//! decryptor starts at that chunk index, and the surplus leading bytes are
//! dropped before anything is sent.
//!
//! The server binds 127.0.0.1 on a random port and every path carries a
//! random per-session token; requests without it get 403, so another local
//! user cannot use it as an open Deezer proxy.

use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::response::Response;
use axum::routing::get;
use axum::Router;

use super::crypto::{track_key, StripeDecryptor, CHUNK_SIZE};
use super::{DeezerClient, Quality, ResolvedStream};
use crate::SoneError;

/// A stream is read at playback speed, so the shared client's 30 s total
/// timeout would cut every track. Overridden per request for the CDN body.
const STREAM_TIMEOUT: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Debug, Clone)]
pub struct ServerHandle {
    pub port: u16,
    token: String,
}

impl ServerHandle {
    pub fn url_for(&self, track_id: u64, format: Quality) -> String {
        format!(
            "http://127.0.0.1:{}/deezer/{}/{}.{}",
            self.port,
            self.token,
            track_id,
            format.ext()
        )
    }
}

struct Ctx {
    client: Arc<DeezerClient>,
    token: String,
}

pub async fn start(client: Arc<DeezerClient>) -> Result<ServerHandle, SoneError> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
    let port = listener.local_addr()?.port();
    let token = uuid::Uuid::new_v4().simple().to_string();
    let ctx = Arc::new(Ctx {
        client,
        token: token.clone(),
    });
    let app = Router::new()
        .route("/deezer/:token/:file", get(serve))
        .with_state(ctx);
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            log::error!("[deezer] stream server exited: {e}");
        }
    });
    log::info!("[deezer] stream server listening on 127.0.0.1:{port}");
    Ok(ServerHandle { port, token })
}

// ---------------------------------------------------------------------------
// Pure helpers (unit-tested)
// ---------------------------------------------------------------------------

/// A single-range `Range` header value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RangeSpec {
    /// `bytes=start-` or `bytes=start-end` (end inclusive).
    From(u64, Option<u64>),
    /// `bytes=-n`: the last n bytes.
    Suffix(u64),
}

/// Parse `bytes=...`. Multi-range and malformed values return `None`, which
/// RFC 9110 lets a server answer with the whole representation.
pub fn parse_range(h: &str) -> Option<RangeSpec> {
    let spec = h.trim().strip_prefix("bytes=")?.trim();
    if spec.contains(',') {
        return None;
    }
    let (a, b) = spec.split_once('-')?;
    let (a, b) = (a.trim(), b.trim());
    if a.is_empty() {
        return b.parse().ok().filter(|&n| n > 0).map(RangeSpec::Suffix);
    }
    let start: u64 = a.parse().ok()?;
    if b.is_empty() {
        return Some(RangeSpec::From(start, None));
    }
    let end: u64 = b.parse().ok()?;
    (end >= start).then_some(RangeSpec::From(start, Some(end)))
}

/// Absolute inclusive `(start, end)` for a spec against a known total.
/// `None` means unsatisfiable (416).
pub fn resolve_range(spec: RangeSpec, total: u64) -> Option<(u64, u64)> {
    if total == 0 {
        return None;
    }
    match spec {
        RangeSpec::From(start, end) => {
            if start >= total {
                return None;
            }
            Some((start, end.map_or(total - 1, |e| e.min(total - 1))))
        }
        RangeSpec::Suffix(n) => Some((total.saturating_sub(n), total - 1)),
    }
}

/// Offset aligned down to its stripe chunk, and that chunk's index.
pub fn align_down(offset: u64) -> (u64, u64) {
    let chunk = offset / CHUNK_SIZE as u64;
    (chunk * CHUNK_SIZE as u64, chunk)
}

/// `bytes a-b/total` → `(a, total)`; `total` may be `*`.
pub fn parse_content_range(h: &str) -> Option<(u64, Option<u64>)> {
    let rest = h.trim().strip_prefix("bytes")?.trim();
    let (range, total) = rest.split_once('/')?;
    let (a, _) = range.split_once('-')?;
    Some((a.trim().parse().ok()?, total.trim().parse().ok()))
}

/// Keep only the bytes of `data` inside the client's window: drop `skip`
/// leading bytes first, then cap at `remaining`. Both are updated.
pub fn window(data: &[u8], skip: &mut u64, remaining: &mut u64) -> Vec<u8> {
    let s = (*skip).min(data.len() as u64) as usize;
    *skip -= s as u64;
    let rest = &data[s..];
    let n = (*remaining).min(rest.len() as u64) as usize;
    *remaining -= n as u64;
    rest[..n].to_vec()
}

fn token_matches(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// `<id>.<ext>` → `(id, ext)`; only the two containers Deezer serves.
fn parse_file(file: &str) -> Option<(u64, &str)> {
    let (id, ext) = file.rsplit_once('.')?;
    let id: u64 = id.parse().ok()?;
    matches!(ext, "flac" | "mp3").then_some((id, ext))
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

fn status(code: StatusCode) -> Response {
    Response::builder()
        .status(code)
        .body(Body::empty())
        .expect("static response")
}

fn upstream_error(e: impl std::fmt::Display) -> Response {
    log::warn!("[deezer] stream upstream failed: {e}");
    status(StatusCode::BAD_GATEWAY)
}

/// Open the CDN body from `aligned` onward. Returns the response, the absolute
/// offset its body starts at, and the file's total size if the CDN said.
async fn open_upstream(
    client: &DeezerClient,
    src: &ResolvedStream,
    aligned: u64,
) -> Result<(reqwest::Response, u64, Option<u64>), (StatusCode, String)> {
    let url = src.url.clone();
    let resp = client
        .http()
        .send(|c| {
            let mut r = c.get(&url).timeout(STREAM_TIMEOUT);
            if aligned > 0 {
                r = r.header(reqwest::header::RANGE, format!("bytes={aligned}-"));
            }
            r
        })
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
    let st = resp.status();
    if st == reqwest::StatusCode::PARTIAL_CONTENT {
        let cr = resp
            .headers()
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(parse_content_range)
            .ok_or((StatusCode::BAD_GATEWAY, "206 without Content-Range".into()))?;
        Ok((resp, cr.0, cr.1))
    } else if st.is_success() {
        let total = resp.content_length();
        Ok((resp, 0, total))
    } else {
        Err((
            StatusCode::from_u16(st.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY),
            format!("CDN answered {st}"),
        ))
    }
}

/// Total size without downloading: the song data's FILESIZE, else a 1-byte
/// ranged probe.
async fn total_size(client: &DeezerClient, src: &ResolvedStream) -> Option<u64> {
    if let Some(s) = src.size {
        return Some(s);
    }
    let url = src.url.clone();
    let resp = client
        .http()
        .send(|c| c.get(&url).header(reqwest::header::RANGE, "bytes=0-0"))
        .await
        .ok()?;
    resp.headers()
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_content_range)
        .and_then(|(_, t)| t)
        .or_else(|| resp.content_length().filter(|&l| l > 1))
}

fn base_headers(b: axum::http::response::Builder, q: Quality) -> axum::http::response::Builder {
    b.header(header::CONTENT_TYPE, q.mime())
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CACHE_CONTROL, "no-store")
}

fn unsatisfiable(total: u64) -> Response {
    Response::builder()
        .status(StatusCode::RANGE_NOT_SATISFIABLE)
        .header(header::CONTENT_RANGE, format!("bytes */{total}"))
        .body(Body::empty())
        .expect("static response")
}

/// Headers for a body covering `[start, end]` of `total`.
fn ranged_head(
    q: Quality,
    ranged: bool,
    start: u64,
    end: u64,
    total: u64,
) -> axum::http::response::Builder {
    let b = base_headers(Response::builder(), q).header(header::CONTENT_LENGTH, end + 1 - start);
    if ranged {
        b.status(StatusCode::PARTIAL_CONTENT).header(
            header::CONTENT_RANGE,
            format!("bytes {start}-{end}/{total}"),
        )
    } else {
        b.status(StatusCode::OK)
    }
}

async fn serve(
    State(ctx): State<Arc<Ctx>>,
    Path((token, file)): Path<(String, String)>,
    method: Method,
    headers: HeaderMap,
) -> Response {
    if !token_matches(&token, &ctx.token) {
        return status(StatusCode::FORBIDDEN);
    }
    let Some((track_id, container)) = parse_file(&file) else {
        return status(StatusCode::NOT_FOUND);
    };
    let spec = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_range);

    let mut src = match ctx.client.stream_source(track_id, container).await {
        Ok(s) => s,
        Err(e) => return upstream_error(e.log_safe()),
    };

    if method == Method::HEAD {
        let Some(total) = total_size(&ctx.client, &src).await else {
            return upstream_error("size unknown");
        };
        let (start, end) = match spec.map(|s| resolve_range(s, total)) {
            Some(Some(r)) => r,
            Some(None) => return unsatisfiable(total),
            None => (0, total.saturating_sub(1)),
        };
        return ranged_head(src.format, spec.is_some(), start, end, total)
            .body(Body::empty())
            .expect("valid headers");
    }

    // A suffix range needs the total before the upstream request.
    let start_hint = match spec {
        Some(RangeSpec::From(s, _)) => s,
        Some(RangeSpec::Suffix(n)) => match total_size(&ctx.client, &src).await {
            Some(t) if t > 0 => t.saturating_sub(n),
            _ => return upstream_error("size unknown"),
        },
        None => 0,
    };
    let (aligned, _) = align_down(start_hint);

    let mut retried = false;
    let (resp, body_offset, total) = loop {
        match open_upstream(&ctx.client, &src, aligned).await {
            Ok(r) => break r,
            // Expired or revoked CDN URL: resolve again once.
            Err((code, msg)) if !retried && matches!(code.as_u16(), 403 | 404 | 410) => {
                log::debug!("[deezer] {msg}; re-resolving track {track_id}");
                retried = true;
                ctx.client.invalidate_stream(track_id);
                src = match ctx.client.stream_source(track_id, container).await {
                    Ok(s) => s,
                    Err(e) => return upstream_error(e.log_safe()),
                };
            }
            // Start past the end of the file.
            Err((StatusCode::RANGE_NOT_SATISFIABLE, _)) => {
                return unsatisfiable(src.size.unwrap_or(0))
            }
            Err((_, msg)) => return upstream_error(msg),
        }
    };
    let Some(total) = total.or(src.size) else {
        return upstream_error("size unknown");
    };
    if body_offset % CHUNK_SIZE as u64 != 0 || body_offset > start_hint {
        return upstream_error("CDN returned a misaligned range");
    }
    let (start, end) = match spec.map(|s| resolve_range(s, total)) {
        Some(Some(r)) => r,
        Some(None) => return unsatisfiable(total),
        None => (0, total.saturating_sub(1)),
    };

    let state = BodyState {
        resp: Some(resp),
        dec: StripeDecryptor::new(track_key(src.key_id), body_offset / CHUNK_SIZE as u64),
        skip: start - body_offset,
        remaining: end + 1 - start,
    };
    let stream = futures_util::stream::unfold(state, next_piece);
    ranged_head(src.format, spec.is_some(), start, end, total)
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| status(StatusCode::INTERNAL_SERVER_ERROR))
}

struct BodyState {
    resp: Option<reqwest::Response>,
    dec: StripeDecryptor,
    skip: u64,
    remaining: u64,
}

async fn next_piece(mut st: BodyState) -> Option<(Result<Bytes, std::io::Error>, BodyState)> {
    loop {
        if st.remaining == 0 {
            return None;
        }
        let resp = st.resp.as_mut()?;
        let produced = match resp.chunk().await {
            Ok(Some(b)) => st.dec.feed(&b),
            Ok(None) => {
                st.resp = None;
                st.dec.finish()
            }
            Err(e) => {
                st.resp = None;
                st.remaining = 0;
                return Some((Err(std::io::Error::other(e)), st));
            }
        };
        let out = window(&produced, &mut st.skip, &mut st.remaining);
        if !out.is_empty() {
            return Some((Ok(Bytes::from(out)), st));
        }
        // Upstream finished and nothing was left to send.
        st.resp.as_ref()?;
    }
}

#[cfg(test)]
mod tests {
    use super::super::crypto::encrypt_stripes;
    use super::*;

    #[test]
    fn deezer_range_parse() {
        assert_eq!(parse_range("bytes=0-"), Some(RangeSpec::From(0, None)));
        assert_eq!(
            parse_range("bytes=5000-9999"),
            Some(RangeSpec::From(5000, Some(9999)))
        );
        assert_eq!(parse_range("bytes=-500"), Some(RangeSpec::Suffix(500)));
        assert_eq!(parse_range("bytes=10-5"), None);
        assert_eq!(parse_range("bytes=0-1,5-6"), None);
        assert_eq!(parse_range("items=0-1"), None);
    }

    #[test]
    fn deezer_range_resolve_and_align() {
        assert_eq!(
            resolve_range(RangeSpec::From(0, None), 10_000),
            Some((0, 9_999))
        );
        assert_eq!(
            resolve_range(RangeSpec::From(100, Some(50_000)), 10_000),
            Some((100, 9_999))
        );
        assert_eq!(resolve_range(RangeSpec::From(10_000, None), 10_000), None);
        assert_eq!(
            resolve_range(RangeSpec::Suffix(500), 10_000),
            Some((9_500, 9_999))
        );
        assert_eq!(
            resolve_range(RangeSpec::Suffix(50_000), 10_000),
            Some((0, 9_999))
        );

        assert_eq!(align_down(0), (0, 0));
        assert_eq!(align_down(2047), (0, 0));
        assert_eq!(align_down(2048), (2048, 1));
        assert_eq!(align_down(6145), (6144, 3));
        assert_eq!(align_down(1_000_000), (999_424, 488));
    }

    #[test]
    fn deezer_content_range_parse() {
        assert_eq!(
            parse_content_range("bytes 6144-99999/100000"),
            Some((6144, Some(100_000)))
        );
        assert_eq!(parse_content_range("bytes 0-0/*"), Some((0, None)));
        assert_eq!(parse_content_range("garbage"), None);
    }

    #[test]
    fn deezer_window_trims_leading_and_caps() {
        let data: Vec<u8> = (0..100).collect();
        let (mut skip, mut rem) = (30, 50);
        let a = window(&data[..20], &mut skip, &mut rem);
        assert!(a.is_empty());
        assert_eq!((skip, rem), (10, 50));
        let b = window(&data[20..], &mut skip, &mut rem);
        assert_eq!(b, (30..80).collect::<Vec<u8>>());
        assert_eq!((skip, rem), (0, 0));
    }

    /// End-to-end over the pure pieces: an arbitrary client range served from
    /// an aligned upstream body equals the same slice of the plaintext.
    #[test]
    fn deezer_ranged_decrypt_matches_plaintext_slice() {
        let key = track_key(3135556);
        let total = CHUNK_SIZE * 9 + 777;
        let plain: Vec<u8> = (0..total).map(|i| (i * 13 + 5) as u8).collect();
        let enc = encrypt_stripes(&key, 0, &plain);
        for (s, e) in [
            (0, total - 1),
            (1, 10),
            (2047, 2049),
            (6145, 15_000),
            (total - 5, total - 1),
            (18_432, total - 1),
        ] {
            let (start, end) =
                resolve_range(RangeSpec::From(s as u64, Some(e as u64)), total as u64).unwrap();
            let (aligned, chunk) = align_down(start);
            let mut dec = StripeDecryptor::new(key, chunk);
            let (mut skip, mut rem) = (start - aligned, end + 1 - start);
            let mut out = Vec::new();
            for piece in enc[aligned as usize..].chunks(1500) {
                out.extend(window(&dec.feed(piece), &mut skip, &mut rem));
            }
            out.extend(window(&dec.finish(), &mut skip, &mut rem));
            assert_eq!(out, plain[s..=e], "range {s}-{e}");
        }
    }

    #[test]
    fn deezer_token_and_file_parsing() {
        assert!(token_matches("abc", "abc"));
        assert!(!token_matches("abc", "abd"));
        assert!(!token_matches("abc", "abcd"));
        assert_eq!(parse_file("3135556.flac"), Some((3135556, "flac")));
        assert_eq!(parse_file("3135556.mp3"), Some((3135556, "mp3")));
        assert_eq!(parse_file("3135556.ogg"), None);
        assert_eq!(parse_file("../etc.flac"), None);
    }

    /// Mock CDN serving `data` with single-range support, like the real one.
    async fn mock_cdn(data: Arc<Vec<u8>>) -> String {
        let app = Router::new().route(
            "/file",
            get(move |headers: HeaderMap| {
                let data = Arc::clone(&data);
                async move {
                    let total = data.len() as u64;
                    let start = headers
                        .get(header::RANGE)
                        .and_then(|v| v.to_str().ok())
                        .and_then(parse_range)
                        .and_then(|s| resolve_range(s, total))
                        .map(|(s, _)| s);
                    match start {
                        Some(s) => Response::builder()
                            .status(StatusCode::PARTIAL_CONTENT)
                            .header(
                                header::CONTENT_RANGE,
                                format!("bytes {s}-{}/{total}", total - 1),
                            )
                            .header(header::CONTENT_LENGTH, total - s)
                            .body(Body::from(data[s as usize..].to_vec()))
                            .unwrap(),
                        None => Response::builder()
                            .header(header::CONTENT_LENGTH, total)
                            .body(Body::from(data.to_vec()))
                            .unwrap(),
                    }
                }
            }),
        );
        let l = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = l.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
        format!("http://127.0.0.1:{port}/file")
    }

    #[tokio::test]
    async fn deezer_loopback_server_serves_decrypted_ranges() {
        let id = 3135556;
        let key = track_key(id);
        let total = CHUNK_SIZE * 20 + 333;
        let plain: Vec<u8> = (0..total).map(|i| (i * 7 + i / 2048) as u8).collect();
        let cdn = mock_cdn(Arc::new(encrypt_stripes(&key, 0, &plain))).await;

        let caps = crate::proxy::HostCaps::assume_all_present();
        let http =
            crate::proxy_http::ProxiedHttp::from_plan(&crate::proxy::ProxyPlan::Direct, &caps);
        let client = Arc::new(DeezerClient::new_for_test(http));
        client.insert_stream_for_test(ResolvedStream {
            track_id: id,
            key_id: id,
            url: cdn,
            format: Quality::Flac,
            size: Some(total as u64),
            gain: None,
            resolved_at: std::time::Instant::now(),
        });
        let url = client.loopback_url(id, Quality::Flac).await.unwrap();
        assert!(url.starts_with("http://127.0.0.1:") && url.ends_with("/3135556.flac"));

        // The test's own requests reuse the shared cell's client: building a
        // client anywhere but proxy_http.rs is banned by tests/source_guards.rs.
        let ua = client.http().0.client().unwrap();

        // Whole file, no Range.
        let r = ua.get(&url).send().await.unwrap();
        assert_eq!(r.status().as_u16(), 200);
        assert_eq!(r.headers()["content-type"], "audio/flac");
        assert_eq!(r.headers()["accept-ranges"], "bytes");
        assert_eq!(r.content_length(), Some(total as u64));
        assert_eq!(r.bytes().await.unwrap().to_vec(), plain);

        // Unaligned ranges, open and closed, and a suffix.
        for (range, s, e) in [
            ("bytes=5000-9999".to_string(), 5000, 9999),
            ("bytes=6145-".to_string(), 6145, total - 1),
            ("bytes=-100".to_string(), total - 100, total - 1),
            (
                format!("bytes={}-{}", total - 1, total + 50),
                total - 1,
                total - 1,
            ),
        ] {
            let r = ua.get(&url).header("Range", &range).send().await.unwrap();
            assert_eq!(r.status().as_u16(), 206, "{range}");
            assert_eq!(
                r.headers()["content-range"].to_str().unwrap(),
                format!("bytes {s}-{e}/{total}"),
                "{range}"
            );
            assert_eq!(r.content_length(), Some((e + 1 - s) as u64), "{range}");
            assert_eq!(r.bytes().await.unwrap().to_vec(), plain[s..=e], "{range}");
        }

        // Past the end.
        let r = ua
            .get(&url)
            .header("Range", format!("bytes={total}-"))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status().as_u16(), 416);

        // HEAD gives the size without a body.
        // (reqwest's content_length() reads the empty body's size hint for a
        // HEAD answer, so check the header itself.)
        let r = ua.head(&url).send().await.unwrap();
        assert_eq!(r.status().as_u16(), 200);
        assert_eq!(
            r.headers()["content-length"].to_str().unwrap(),
            total.to_string()
        );

        // Wrong token.
        let bad = url.replace("/deezer/", "/deezer/x");
        assert_eq!(ua.get(&bad).send().await.unwrap().status().as_u16(), 403);
    }
}

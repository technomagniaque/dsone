//! Deezer Cloud Queue synchronization (KMP QueueSync / CBQL over Pipe GraphQL).
//!
//! Shares the exact same `pipe.deezer.com/api` GraphQL operations used by the
//! official Deezer Android, iOS, Web, and Desktop clients (`KmpQsQueueState`,
//! `KmpQsQueueChunkList`, `KmpQsPushQueueState`) so users can resume playback
//! seamlessly between their phone and `dsone` in both directions.
//!
//! Authentication exchanges the session `arl` cookie for a short-lived Pipe JWT
//! via `POST https://auth.deezer.com/login/arl?jo=p&rto=c&i=c`, routed through
//! the shared `Http(ProxiedHttp)` client.

use base64::Engine as _;
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, COOKIE, ORIGIN};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::deezer::api::Http;
use crate::deezer::models::DzTrack;
use crate::SoneError;

pub const AUTH_ARL_PAYLOAD_URL: &str = "https://auth.deezer.com/login/arl?jo=p&rto=c&i=p";
pub const AUTH_ARL_COOKIE_URL: &str = "https://auth.deezer.com/login/arl?jo=p&rto=c&i=c";
pub const PIPE_URL: &str = "https://pipe.deezer.com/api";
const DEEZER_ORIGIN: &str = "https://www.deezer.com";
const MAX_SYNC_TRACKS: usize = 500;

const QUEUE_FRAGMENTS: &str = r#"
fragment QueueFragment on QueueChunk {
  id
  contexts { id content }
  queueMetadata { queueId chunkMetadataList { id } }
  mediaList { id type contextId localQueueItemId }
}
fragment QueueStateFragment on QueueState {
  position
  repeatMode
  version
  lastClientDetails { userAgent localQueueID locallyUpdatedAt }
  queueConfiguration
  originalQueue { __typename ...QueueFragment }
  queue { __typename ...QueueFragment }
}
"#;

const QUERY_QUEUE_STATE: &str = r#"query KmpQsQueueState {
  me {
    lastKnownQueueState {
      __typename
      ...QueueStateFragment
    }
  }
}
fragment QueueFragment on QueueChunk {
  id
  contexts { id content }
  queueMetadata { queueId chunkMetadataList { id } }
  mediaList { id type contextId localQueueItemId }
}
fragment QueueStateFragment on QueueState {
  position
  repeatMode
  version
  lastClientDetails { userAgent localQueueID locallyUpdatedAt }
  queueConfiguration
  originalQueue { __typename ...QueueFragment }
  queue { __typename ...QueueFragment }
}"#;

const QUERY_QUEUE_CHUNKS: &str = r#"query KmpQsQueueChunkList($chunkId: [ID!]!, $queueId: ID!) {
  queueChunksByIds(chunksIds: $chunkId, queueId: $queueId) {
    __typename
    ...QueueFragment
  }
}
fragment QueueFragment on QueueChunk {
  id
  contexts { id content }
  queueMetadata { queueId chunkMetadataList { id } }
  mediaList { id type contextId localQueueItemId }
}"#;

const MUTATION_PUSH_QUEUE: &str = r#"mutation KmpQsPushQueueState(
  $clientDetails: PushQueueStateClientDetailsInput!,
  $mediaList: [PushQueueMediaInput!]!,
  $context: [PushQueueContextInput!]!,
  $position: Int!,
  $previousVersion: String,
  $queueConfiguration: String!,
  $repeatMode: RepeatModeInput!
) {
  pushQueueState(input: {
    clientDetails: $clientDetails
    position: $position
    previousVersion: $previousVersion
    queueConfiguration: $queueConfiguration
    repeatMode: $repeatMode
    contexts: $context
    mediaList: $mediaList
  }) {
    __typename
    ... on PushQueueStateOutput { version updatedAt }
    ... on PushQueueStateError {
      isBadRequest
      conflict { __typename current { __typename ...QueueStateFragment } }
    }
  }
}
fragment QueueFragment on QueueChunk {
  id
  contexts { id content }
  queueMetadata { queueId chunkMetadataList { id } }
  mediaList { id type contextId localQueueItemId }
}
fragment QueueStateFragment on QueueState {
  position
  repeatMode
  version
  lastClientDetails { userAgent localQueueID locallyUpdatedAt }
  queueConfiguration
  originalQueue { __typename ...QueueFragment }
  queue { __typename ...QueueFragment }
}"#;

/// Cached Pipe JWT token. Debug redacts the raw JWT string.
#[derive(Clone)]
pub struct PipeJwt {
    pub token: String,
    /// Approximate local Unix expiry (`exp + clock_skew_secs`), matching the
    /// official Deezer Desktop client's `approximateClientExpire` calculation.
    pub exp_local_secs: u64,
    /// `local_now - jwt.iat` in seconds so timestamps sent to Deezer match
    /// Deezer's server clock even when the local system clock is skewed.
    pub clock_skew_secs: i64,
}

impl std::fmt::Debug for PipeJwt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PipeJwt")
            .field("exp_local_secs", &self.exp_local_secs)
            .field("clock_skew_secs", &self.clock_skew_secs)
            .field("token", &"<redacted>")
            .finish()
    }
}

impl PipeJwt {
    pub fn is_fresh(&self) -> bool {
        crate::now_secs() + 60 < self.exp_local_secs
    }

    /// Current Unix timestamp adjusted to Deezer's server clock.
    pub fn server_now_secs(&self) -> u64 {
        let local = crate::now_secs() as i64;
        (local - self.clock_skew_secs).max(0) as u64
    }
}

/// Synchronized cloud queue returned to the frontend (tracks already hydrated).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DzCloudQueueState {
    pub tracks: Vec<DzTrack>,
    pub position: usize,
    /// `0` = off, `1` = repeat all, `2` = repeat one (matches SONE's `repeatAtom`).
    pub repeat_mode: u8,
    pub shuffled: bool,
    pub version: String,
    pub updated_at: Option<String>,
    pub local_queue_id: Option<String>,
    pub source_user_agent: Option<String>,
}

/// Input from the frontend when pushing dsone's active Deezer queue to Cloud Queue.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DzPushQueueInput {
    pub track_ids: Vec<u64>,
    pub position: usize,
    /// `0` = off, `1` = repeat all, `2` = repeat one.
    pub repeat_mode: u8,
    pub shuffled: bool,
    pub previous_version: Option<String>,
    pub local_queue_id: Option<String>,
    pub context_id: Option<String>,
    pub context_type: Option<String>,
}

/// Result of a cloud queue push.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DzPushQueueResult {
    pub version: String,
    pub updated_at: Option<String>,
    pub local_queue_id: String,
}

/// Raw parsed cloud queue before track hydration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCloudQueue {
    pub track_ids: Vec<u64>,
    pub position: usize,
    pub repeat_mode: u8,
    pub shuffled: bool,
    pub version: String,
    pub updated_at: Option<String>,
    pub local_queue_id: Option<String>,
    pub source_user_agent: Option<String>,
    pub queue_id: Option<String>,
    pub loaded_chunk_id: Option<String>,
    pub extra_chunk_ids: Vec<String>,
}

/// Decode `(iat, exp)` Unix timestamps from a JWT payload without verifying signatures.
pub fn parse_jwt_claims(jwt: &str) -> Option<(Option<u64>, Option<u64>)> {
    let payload_b64 = jwt.split('.').nth(1)?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(payload_b64))
        .ok()?;
    let v: Value = serde_json::from_slice(&decoded).ok()?;
    let iat = v.get("iat").and_then(Value::as_u64);
    let exp = v.get("exp").and_then(Value::as_u64);
    Some((iat, exp))
}

/// Decode the `exp` Unix timestamp from a JWT payload without verifying signatures.
#[cfg(test)]
pub fn parse_jwt_exp(jwt: &str) -> Option<u64> {
    parse_jwt_claims(jwt)?.1
}

fn build_pipe_jwt(token: String, local_now: u64) -> PipeJwt {
    let (iat, exp) = parse_jwt_claims(&token).unwrap_or((None, None));
    let clock_skew_secs = iat.map(|t| (local_now as i64) - (t as i64)).unwrap_or(0);
    let exp_local_secs = match exp {
        Some(e) => ((e as i64) + clock_skew_secs).max(0) as u64,
        None => local_now + 10 * 60,
    };
    PipeJwt {
        token,
        exp_local_secs,
        clock_skew_secs,
    }
}

/// Exchange the user's `arl` for a Pipe API JWT.
///
/// Uses the payload endpoint (`?jo=p&rto=c&i=p` with JSON `{"arl": ...}`) first
/// — matching official Deezer Desktop `refreshArlLogin` — because Akamai
/// returns `411 Length Required` on bodyless cookie-only POST requests. Falls
/// back to cookie mode (`?jo=p&rto=c&i=c`) with an explicit empty body if needed.
pub async fn fetch_pipe_jwt(http: &Http, arl: &str) -> Result<PipeJwt, SoneError> {
    let payload = json!({ "arl": arl });
    let resp = http
        .send(|c| {
            c.post(AUTH_ARL_PAYLOAD_URL)
                .header(ORIGIN, DEEZER_ORIGIN)
                .header(ACCEPT, "application/json")
                .header(CONTENT_TYPE, "application/json")
                .json(&payload)
        })
        .await?;

    let resp = if resp.status().is_success() {
        resp
    } else {
        let first_status = resp.status();
        log::warn!(
            "[deezer-sync] auth/login/arl payload mode returned {first_status}, trying cookie fallback"
        );
        let cookie = format!("arl={arl}");
        http.send(|c| {
            c.post(AUTH_ARL_COOKIE_URL)
                .header(COOKIE, &cookie)
                .header(ORIGIN, DEEZER_ORIGIN)
                .header(ACCEPT, "application/json")
                .body("")
        })
        .await?
    };

    let status = resp.status();
    if !status.is_success() {
        let err_snippet: String = resp
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(180)
            .collect();
        log::warn!("[deezer-sync] auth/login/arl failed ({status}): {err_snippet}");
        return Err(SoneError::Api {
            status: status.as_u16(),
            body: format!("deezer auth/login/arl failed ({status}): {err_snippet}"),
        });
    }
    let body: Value = resp
        .json()
        .await
        .map_err(|e| SoneError::Parse(format!("deezer auth json: {e}")))?;
    let token = body
        .get("jwt")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| SoneError::Parse("deezer auth/login/arl: missing jwt".into()))?
        .to_string();
    let jwt = build_pipe_jwt(token, crate::now_secs());
    log::info!(
        "[deezer-sync] obtained Pipe JWT (clock_skew={}s)",
        jwt.clock_skew_secs
    );
    Ok(jwt)
}

/// Execute one GraphQL operation on `https://pipe.deezer.com/api`.
pub async fn pipe_graphql(
    http: &Http,
    jwt: &str,
    operation_name: &str,
    query: &str,
    variables: Value,
) -> Result<Value, SoneError> {
    let auth = format!("Bearer {jwt}");
    let payload = json!({
        "operationName": operation_name,
        "query": query,
        "variables": variables,
    });
    let resp = http
        .send(|c| {
            c.post(PIPE_URL)
                .header(AUTHORIZATION, &auth)
                .header(CONTENT_TYPE, "application/json")
                .header(ACCEPT, "application/json")
                .header("Accept-Language", "en")
                .header(ORIGIN, DEEZER_ORIGIN)
                .json(&payload)
        })
        .await?;
    let status = resp.status();
    if status.as_u16() == 401 {
        return Err(SoneError::NotAuthenticated);
    }
    if !status.is_success() {
        let text: String = resp
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(300)
            .collect();
        log::warn!("[deezer-sync] pipe {operation_name} HTTP {status}: {text}");
        return Err(SoneError::Api {
            status: status.as_u16(),
            body: format!("deezer pipe {operation_name}: {text}"),
        });
    }
    let body: Value = resp
        .json()
        .await
        .map_err(|e| SoneError::Parse(format!("deezer pipe json: {e}")))?;
    if let Some(errs) = body.get("errors").and_then(Value::as_array) {
        if !errs.is_empty() {
            let msg = errs
                .iter()
                .filter_map(|e| e.get("message").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("; ");
            let err_types = errs
                .iter()
                .filter_map(|e| {
                    e.get("type")
                        .and_then(Value::as_str)
                        .or_else(|| e.pointer("/extensions/type").and_then(Value::as_str))
                })
                .collect::<Vec<_>>()
                .join(",");
            if msg.to_ascii_lowercase().contains("unauth")
                || msg.to_ascii_lowercase().contains("jwt")
                || err_types.contains("JwtToken")
            {
                return Err(SoneError::NotAuthenticated);
            }
            log::warn!(
                "[deezer-sync] pipe {operation_name} GraphQL error ({err_types}): {msg}"
            );
            return Err(SoneError::Api {
                status: 400,
                body: format!("deezer pipe {operation_name}: {msg}"),
            });
        }
    }
    Ok(body.get("data").cloned().unwrap_or(Value::Null))
}

fn repeat_from_str(s: Option<&str>) -> u8 {
    match s {
        Some("ALL") => 1,
        Some("ONE") => 2,
        _ => 0,
    }
}

pub fn repeat_to_str(mode: u8) -> &'static str {
    match mode {
        1 => "ALL",
        2 => "ONE",
        _ => "NONE",
    }
}

/// Extract track IDs from a `QueueChunk`'s `mediaList`, adjusting the target
/// index when non-track items (episodes, live streams) precede it.
fn extract_chunk_tracks(
    chunk: &Value,
    raw_position: usize,
    raw_offset: usize,
) -> (Vec<u64>, Option<usize>, usize) {
    let list = chunk
        .get("mediaList")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let mut ids = Vec::new();
    let mut mapped_pos = None;
    for (i, item) in list.iter().enumerate() {
        let global_idx = raw_offset + i;
        let is_track = item
            .get("type")
            .and_then(Value::as_str)
            .is_none_or(|t| t == "TRACK");
        if is_track {
            if let Some(id) = item
                .get("id")
                .and_then(Value::as_str)
                .and_then(|s| s.parse::<u64>().ok())
                .or_else(|| item.get("id").and_then(Value::as_u64))
            {
                if global_idx == raw_position {
                    mapped_pos = Some(ids.len());
                }
                ids.push(id);
            }
        } else if global_idx == raw_position {
            mapped_pos = Some(ids.len());
        }
    }
    (ids, mapped_pos, list.len())
}

/// Parse `data.me.lastKnownQueueState` from `KmpQsQueueState`.
pub fn parse_queue_state(data: &Value) -> Option<ParsedCloudQueue> {
    let state = data.pointer("/me/lastKnownQueueState")?;
    if state.is_null() {
        return None;
    }
    let queue = state.get("queue")?;
    let version = state.get("version").and_then(Value::as_str)?.to_string();
    let raw_position = state
        .get("position")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let repeat_mode = repeat_from_str(state.get("repeatMode").and_then(Value::as_str));
    let shuffled = state
        .get("originalQueue")
        .is_some_and(|oq| !oq.is_null());

    let details = state.get("lastClientDetails");
    let updated_at = details
        .and_then(|d| d.get("locallyUpdatedAt"))
        .and_then(Value::as_str)
        .map(String::from);
    let local_queue_id = details
        .and_then(|d| d.get("localQueueID"))
        .and_then(Value::as_str)
        .map(String::from);
    let source_user_agent = details
        .and_then(|d| d.get("userAgent"))
        .and_then(Value::as_str)
        .map(String::from);

    let loaded_chunk_id = queue.get("id").and_then(Value::as_str).map(String::from);
    let queue_id = queue
        .pointer("/queueMetadata/queueId")
        .and_then(Value::as_str)
        .map(String::from);

    let extra_chunk_ids: Vec<String> = queue
        .pointer("/queueMetadata/chunkMetadataList")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c.get("id").and_then(Value::as_str))
                .filter(|id| loaded_chunk_id.as_deref() != Some(*id))
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();

    let (track_ids, mapped_pos, _) = extract_chunk_tracks(queue, raw_position, 0);
    let position = mapped_pos
        .unwrap_or(raw_position)
        .min(track_ids.len().saturating_sub(1));

    Some(ParsedCloudQueue {
        track_ids,
        position,
        repeat_mode,
        shuffled,
        version,
        updated_at,
        local_queue_id,
        source_user_agent,
        queue_id,
        loaded_chunk_id,
        extra_chunk_ids,
    })
}

/// Pull the latest cloud queue state (including extra chunks if needed).
pub async fn pull_raw_queue(
    http: &Http,
    jwt: &str,
) -> Result<Option<ParsedCloudQueue>, SoneError> {
    let data = pipe_graphql(http, jwt, "KmpQsQueueState", QUERY_QUEUE_STATE, json!({})).await?;
    let Some(mut parsed) = parse_queue_state(&data) else {
        return Ok(None);
    };

    // If the queue spans multiple chunks and we haven't hit MAX_SYNC_TRACKS, fetch
    // the remaining chunks in order so large playlists/queues resume accurately.
    if !parsed.extra_chunk_ids.is_empty() && parsed.track_ids.len() < MAX_SYNC_TRACKS {
        if let Some(queue_id) = parsed.queue_id.clone() {
            let chunk_ids: Vec<String> = parsed.extra_chunk_ids.iter().take(10).cloned().collect();
            if let Ok(chunks_data) = pipe_graphql(
                http,
                jwt,
                "KmpQsQueueChunkList",
                QUERY_QUEUE_CHUNKS,
                json!({ "chunkId": chunk_ids, "queueId": queue_id }),
            )
            .await
            {
                if let Some(chunks) = chunks_data
                    .get("queueChunksByIds")
                    .and_then(Value::as_array)
                {
                    let raw_pos = data
                        .pointer("/me/lastKnownQueueState/position")
                        .and_then(Value::as_u64)
                        .unwrap_or(0) as usize;
                    let all_chunk_ids: Vec<String> = data
                        .pointer("/me/lastKnownQueueState/queue/queueMetadata/chunkMetadataList")
                        .and_then(Value::as_array)
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|c| c.get("id").and_then(Value::as_str).map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    let initial_chunk = data.pointer("/me/lastKnownQueueState/queue");
                    let mut all_ids = Vec::new();
                    let mut raw_offset = 0usize;
                    let mut found_pos = None;
                    for cid in &all_chunk_ids {
                        let chunk = if parsed.loaded_chunk_id.as_deref() == Some(cid.as_str()) {
                            initial_chunk
                        } else {
                            chunks
                                .iter()
                                .find(|c| c.get("id").and_then(Value::as_str) == Some(cid.as_str()))
                        };
                        if let Some(ch) = chunk {
                            let (ids, mapped, count) =
                                extract_chunk_tracks(ch, raw_pos, raw_offset);
                            if let Some(p) = mapped {
                                found_pos = Some(all_ids.len() + p);
                            }
                            all_ids.extend(ids);
                            raw_offset += count;
                            if found_pos.is_some() && all_ids.len() >= MAX_SYNC_TRACKS {
                                break;
                            }
                        }
                    }
                    if !all_ids.is_empty() {
                        let pos = found_pos
                            .unwrap_or(parsed.position)
                            .min(all_ids.len().saturating_sub(1));
                        // Window around `pos` if larger than MAX_SYNC_TRACKS so the active track is preserved.
                        if all_ids.len() > MAX_SYNC_TRACKS {
                            let half = MAX_SYNC_TRACKS / 2;
                            let start = pos.saturating_sub(half).min(all_ids.len() - MAX_SYNC_TRACKS);
                            parsed.track_ids = all_ids[start..start + MAX_SYNC_TRACKS].to_vec();
                            parsed.position = pos - start;
                        } else {
                            parsed.track_ids = all_ids;
                            parsed.position = pos;
                        }
                    }
                }
            }
        }
    }

    parsed.track_ids.truncate(MAX_SYNC_TRACKS);
    if parsed.position >= parsed.track_ids.len() && !parsed.track_ids.is_empty() {
        parsed.position = parsed.track_ids.len() - 1;
    }

    log::info!(
        "[deezer-sync] pulled queue version={} tracks={} pos={} source={:?}",
        parsed.version,
        parsed.track_ids.len(),
        parsed.position,
        parsed.source_user_agent
    );
    Ok(Some(parsed))
}

/// Format Unix epoch seconds into an ISO-8601 UTC timestamp (`YYYY-MM-DDTHH:MM:SS.000Z`).
pub fn format_iso8601_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let hour = rem / 3_600;
    let min = (rem % 3_600) / 60;
    let sec = rem % 60;

    // Howard Hinnant's civil_from_days algorithm (days since 1970-01-01).
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };

    format!("{year:04}-{m:02}-{d:02}T{hour:02}:{min:02}:{sec:02}.000Z")
}

/// Map dsone's internal playback source type to an official Deezer `ctxT`
/// value recognized by Deezer Android, iOS and Web clients.
pub fn map_deezer_context_type(raw: Option<&str>) -> &'static str {
    match raw {
        Some("deezer-album") | Some("album") | Some("album_page") => "album_page",
        Some("deezer-artist") | Some("artist") | Some("artist_page") => "artist_page",
        Some("deezer-flow") | Some("flow") => "flow",
        Some("deezer-favorites") | Some("favorites") | Some("loved") => "loved",
        Some("deezer-history") | Some("history_page") => "history_page",
        Some("deezer-track-radio") | Some("radio_page") => "radio_page",
        _ => "playlist_page",
    }
}

/// Build the GraphQL variables object for `KmpQsPushQueueState`.
pub fn build_push_variables(
    input: &DzPushQueueInput,
    local_queue_id: &str,
    now_iso: &str,
) -> Value {
    let ctx_uuid = uuid::Uuid::new_v4().to_string();
    let ctx_id = input
        .context_id
        .as_deref()
        .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
        .unwrap_or("0");
    let ctx_type = map_deezer_context_type(input.context_type.as_deref());

    let queue_config = json!({
        "allowEdit": true,
        "allowShuffle": true,
        "allowedRepeatModes": ["ALL", "ONE"],
        "ctxId": ctx_id,
        "ctxT": ctx_type,
        "ctxC": Value::Null,
        "lt": "MOD",
        "version": 2,
        "queueType": "basic",
    })
    .to_string();

    let media_ctx_content = json!({
        "ctxId": ctx_id,
        "ctxT": ctx_type,
        "ctxC": Value::Null,
        "lt": "MOD",
        "rawUrlProps": Value::Null,
        "pipeTrackType": "full",
        "version": 2,
    })
    .to_string();

    // Window around `position` when the queue exceeds MAX_SYNC_TRACKS so the
    // currently playing track is always preserved inside the synced window.
    let total = input.track_ids.len();
    let (start, slice) = if total <= MAX_SYNC_TRACKS {
        (0, &input.track_ids[..])
    } else {
        let half = MAX_SYNC_TRACKS / 2;
        let start = input
            .position
            .saturating_sub(half)
            .min(total - MAX_SYNC_TRACKS);
        (start, &input.track_ids[start..start + MAX_SYNC_TRACKS])
    };
    let adjusted_pos = input
        .position
        .saturating_sub(start)
        .min(slice.len().saturating_sub(1));

    let media_list: Vec<Value> = slice
        .iter()
        .enumerate()
        .map(|(idx, id)| {
            json!({
                "id": id.to_string(),
                "type": "TRACK",
                "contextId": ctx_uuid,
                "localQueueItemId": uuid::Uuid::new_v4().to_string(),
                "unshuffledPosition": idx,
            })
        })
        .collect();

    let mut vars = json!({
        "clientDetails": {
            "localQueueID": local_queue_id,
            "locallyUpdatedAt": now_iso,
        },
        "mediaList": media_list,
        "context": [{
            "id": ctx_uuid,
            "content": media_ctx_content,
        }],
        "position": adjusted_pos,
        "queueConfiguration": queue_config,
        "repeatMode": repeat_to_str(input.repeat_mode),
    });
    if let Some(prev) = input
        .previous_version
        .as_deref()
        .filter(|s| !s.is_empty())
    {
        vars["previousVersion"] = Value::String(prev.to_string());
    }
    vars
}

/// Push dsone's active Deezer queue state to Deezer Cloud Queue. Retries once
/// automatically if Deezer returns a version conflict with the server's current
/// `version`.
pub async fn push_queue_state(
    http: &Http,
    jwt: &PipeJwt,
    input: &DzPushQueueInput,
) -> Result<DzPushQueueResult, SoneError> {
    if input.track_ids.is_empty() {
        return Err(SoneError::Api {
            status: 400,
            body: "deezer queue sync: cannot push empty queue".into(),
        });
    }
    let local_queue_id = input
        .local_queue_id
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let now_iso = format_iso8601_utc(jwt.server_now_secs());

    let mut attempt_input = input.clone();
    for attempt in 0..2 {
        let vars = build_push_variables(&attempt_input, &local_queue_id, &now_iso);
        let data = pipe_graphql(
            http,
            &jwt.token,
            "KmpQsPushQueueState",
            MUTATION_PUSH_QUEUE,
            vars,
        )
        .await?;
        let res = data.get("pushQueueState").unwrap_or(&Value::Null);
        let typename = res.get("__typename").and_then(Value::as_str).unwrap_or("");
        if typename == "PushQueueStateOutput" || res.get("version").is_some() {
            let version = res
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let updated_at = res
                .get("updatedAt")
                .and_then(Value::as_str)
                .map(String::from);
            log::info!(
                "[deezer-sync] pushed queue version={} tracks={} pos={}",
                version,
                input.track_ids.len(),
                input.position
            );
            return Ok(DzPushQueueResult {
                version,
                updated_at,
                local_queue_id,
            });
        }
        // On version conflict, adopt the server's latest version and retry once.
        if attempt == 0 {
            if let Some(conflict_ver) = res
                .pointer("/conflict/current/version")
                .and_then(Value::as_str)
            {
                log::info!(
                    "[deezer-sync] push conflict, retrying with server version={conflict_ver}"
                );
                attempt_input.previous_version = Some(conflict_ver.to_string());
                continue;
            }
        }
        let bad_req = res
            .get("isBadRequest")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        log::warn!("[deezer-sync] pushQueueState rejected ({typename}): {res}");
        return Err(SoneError::Api {
            status: if bad_req { 400 } else { 409 },
            body: format!("deezer pushQueueState rejected ({typename}): {res}"),
        });
    }
    Err(SoneError::Api {
        status: 409,
        body: "deezer pushQueueState conflict retry exhausted".into(),
    })
}

// Unused constant kept as reference for schema maintenance.
const _: &str = QUEUE_FRAGMENTS;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jwt_exp_decodes_base64url_payload() {
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"sub":"971921306","iat":1700000000,"exp":1900000000}"#);
        let jwt = format!("eyJhbGciOiJIUzI1NiJ9.{payload}.sig");
        assert_eq!(parse_jwt_exp(&jwt), Some(1_900_000_000));
        assert_eq!(parse_jwt_exp("invalid"), None);
    }

    #[test]
    fn iso8601_utc_formats_known_epochs() {
        assert_eq!(format_iso8601_utc(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(
            format_iso8601_utc(1_795_290_000),
            "2026-11-21T19:40:00.000Z"
        );
    }

    #[test]
    fn parse_queue_state_extracts_tracks_and_remaps_position() {
        let data = json!({
            "me": {
                "lastKnownQueueState": {
                    "position": 2,
                    "repeatMode": "ALL",
                    "version": "v42",
                    "lastClientDetails": {
                        "userAgent": "Deezer/8.0 Android",
                        "localQueueID": "q-android",
                        "locallyUpdatedAt": "2026-11-21T19:30:00.000Z"
                    },
                    "queueConfiguration": "{}",
                    "originalQueue": { "id": "orig-1" },
                    "queue": {
                        "id": "chunk-0",
                        "contexts": [],
                        "queueMetadata": {
                            "queueId": "qid-1",
                            "chunkMetadataList": [{ "id": "chunk-0" }, { "id": "chunk-1" }]
                        },
                        "mediaList": [
                            { "id": "101", "type": "TRACK" },
                            { "id": "999", "type": "EPISODE" },
                            { "id": "202", "type": "TRACK" },
                            { "id": "303", "type": "TRACK" }
                        ]
                    }
                }
            }
        });
        let parsed = parse_queue_state(&data).expect("parsed queue");
        assert_eq!(parsed.track_ids, vec![101, 202, 303]);
        // Raw position 2 was track 202, which is index 1 after skipping the episode.
        assert_eq!(parsed.position, 1);
        assert_eq!(parsed.repeat_mode, 1);
        assert!(parsed.shuffled);
        assert_eq!(parsed.version, "v42");
        assert_eq!(parsed.extra_chunk_ids, vec!["chunk-1"]);
    }

    #[test]
    fn build_push_variables_produces_valid_kmp_v2_payload() {
        let input = DzPushQueueInput {
            track_ids: vec![10, 20, 30],
            position: 1,
            repeat_mode: 2,
            shuffled: false,
            previous_version: Some("v42".into()),
            local_queue_id: Some("local-1".into()),
            context_id: Some("12345".into()),
            context_type: Some("playlist_page".into()),
        };
        let vars = build_push_variables(&input, "local-1", "2026-11-21T19:40:00.000Z");
        assert_eq!(vars["position"], 1);
        assert_eq!(vars["repeatMode"], "ONE");
        assert_eq!(vars["previousVersion"], "v42");
        assert_eq!(vars["mediaList"].as_array().unwrap().len(), 3);
        assert_eq!(vars["mediaList"][1]["id"], "20");
        assert_eq!(vars["mediaList"][1]["type"], "TRACK");
        let cfg: Value =
            serde_json::from_str(vars["queueConfiguration"].as_str().unwrap()).unwrap();
        assert_eq!(cfg["version"], 2);
        assert_eq!(cfg["queueType"], "basic");
        assert_eq!(cfg["ctxT"], "playlist_page");
    }

    #[test]
    fn jwt_clock_skew_aligns_expiry_and_server_time() {
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"sub":"42","iat":1700000000,"exp":1700000900}"#);
        let jwt_str = format!("eyJhbGciOiJIUzI1NiJ9.{payload}.sig");
        // Simulate local clock 1000s ahead of server clock.
        let jwt = build_pipe_jwt(jwt_str, 1_700_001_000);
        assert_eq!(jwt.clock_skew_secs, 1000);
        assert_eq!(jwt.exp_local_secs, 1_700_001_900);
    }

    #[test]
    fn context_types_map_to_official_deezer_names() {
        assert_eq!(map_deezer_context_type(Some("deezer-album")), "album_page");
        assert_eq!(map_deezer_context_type(Some("deezer-flow")), "flow");
        assert_eq!(map_deezer_context_type(Some("deezer-favorites")), "loved");
        assert_eq!(map_deezer_context_type(Some("deezer-playlist")), "playlist_page");
        assert_eq!(map_deezer_context_type(None), "playlist_page");
    }
}

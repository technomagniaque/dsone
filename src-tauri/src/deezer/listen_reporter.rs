//! Continuous Deezer playback telemetry (`log.listen`) for Flow & recommendations.
//!
//! Mirrors official Deezer Desktop (`sendLog(logTrack, logStartedTrack())`):
//! - Announces every newly started Deezer track immediately via `next_media`.
//! - Tracks real active listening time (`lt`, excluding pauses), `stat.pause`,
//!   and `stat.seek`.
//! - Sends a 30-second milestone report once active listening crosses Deezer's
//!   official 30 s stream threshold so history and recommendations stay fresh
//!   even if playback is paused for a long time or the app exits unexpectedly.
//! - Sends the final report with exact `lt` on track completion, gapless advance,
//!   skip, stop, or app shutdown so Deezer's algorithm distinguishes full listens
//!   (strong positive signal) from quick skips (negative signal).

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::deezer::queue_sync::map_deezer_context_type;
use crate::deezer::DeezerClient;

const MILESTONE_SECS: u64 = 30;

#[derive(Debug, Clone)]
pub struct DeezerTrackStart {
    pub track_id: u64,
    pub duration_secs: u64,
    pub source_type: Option<String>,
    pub source_id: Option<String>,
    pub is_shuffle: bool,
    pub repeat_mode: u8,
}

#[derive(Debug)]
struct ActiveListen {
    track_id: u64,
    duration_secs: u64,
    started_unix: u64,
    stream_id: String,
    format: String,
    context_id: String,
    context_type: &'static str,
    player_type: u8, // 1 = radio/flow, 0 = on-demand
    is_shuffle: bool,
    repeat_mode: u8,
    accumulated_secs: f64,
    resumed_at: Option<Instant>,
    pause_count: u32,
    seek_count: u32,
    milestone_sent: bool,
    final_sent: bool,
}

impl ActiveListen {
    fn listened_secs(&self, now: Instant) -> u64 {
        let live = self
            .resumed_at
            .map(|t| now.saturating_duration_since(t).as_secs_f64())
            .unwrap_or(0.0);
        let total = (self.accumulated_secs + live).round().max(0.0) as u64;
        if self.duration_secs > 0 {
            total.min(self.duration_secs)
        } else {
            total
        }
    }

    fn pause(&mut self, now: Instant) {
        if let Some(started) = self.resumed_at.take() {
            self.accumulated_secs += now.saturating_duration_since(started).as_secs_f64();
            self.pause_count = self.pause_count.saturating_add(1);
        }
    }

    fn resume(&mut self, now: Instant) {
        if self.resumed_at.is_none() {
            self.resumed_at = Some(now);
        }
    }

    fn to_params_json(&self, listened_secs: u64) -> Value {
        let mut params = json!({
            "media": {
                "id": self.track_id.to_string(),
                "type": "song",
                "format": self.format,
            },
            "type": self.player_type,
            "stat": {
                "seek": self.seek_count,
                "pause": self.pause_count,
                "sync": 0,
            },
            "lt": listened_secs,
            "ctxt": {
                "id": self.context_id,
                "t": self.context_type,
            },
            "dev": { "v": "7.1.140", "t": 0 },
            "ls": [],
            "ts_listen": self.started_unix,
            "is_shuffle": self.is_shuffle,
            "stream_id": self.stream_id,
        });
        match self.repeat_mode {
            1 => params["repeat_type"] = json!("repeat_all"),
            2 => params["repeat_type"] = json!("repeat_one"),
            _ => {}
        }
        params
    }
}

/// Build the `log.listen` gw-light body matching Deezer Desktop's `sendLog`.
pub fn build_log_listen_body(params: Option<Value>, next_track_id: Option<u64>) -> Value {
    let mut body = serde_json::Map::new();
    if let Some(p) = params {
        body.insert("params".into(), p);
    }
    if let Some(next_id) = next_track_id {
        body.insert(
            "next_media".into(),
            json!({
                "media": {
                    "id": next_id.to_string(),
                    "type": "song",
                }
            }),
        );
    }
    Value::Object(body)
}

fn is_radio_context(ctx_type: &str) -> bool {
    matches!(ctx_type, "flow" | "radio_page")
}

#[derive(Clone)]
pub struct DeezerListenReporter {
    deezer: Arc<DeezerClient>,
    active: Arc<Mutex<Option<ActiveListen>>>,
}

impl DeezerListenReporter {
    pub fn new(deezer: Arc<DeezerClient>) -> Self {
        Self {
            deezer,
            active: Arc::new(Mutex::new(None)),
        }
    }

    /// Record the start of a Deezer track (or close out an active Deezer track
    /// when playback switches to a non-Deezer track by passing `None`).
    pub async fn on_track_started(&self, next: Option<DeezerTrackStart>) {
        if !self.deezer.is_logged_in() {
            *self.active.lock().await = None;
            return;
        }
        let now = Instant::now();
        let next_id = next.as_ref().map(|n| n.track_id);

        let outgoing_payload: Option<(u64, u64, &'static str, String, Value)> = {
            let mut guard = self.active.lock().await;
            let prev = guard.take().and_then(|s| {
                if s.final_sent {
                    return None;
                }
                let lt = s.listened_secs(now);
                let p = s.to_params_json(lt);
                Some((s.track_id, lt, s.context_type, s.format, p))
            });

            if let Some(n) = next {
                let ctx_type = map_deezer_context_type(n.source_type.as_deref());
                let ctx_id = n
                    .source_id
                    .as_deref()
                    .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
                    .unwrap_or("0")
                    .to_string();
                let format = self
                    .deezer
                    .cached_stream_format(n.track_id)
                    .unwrap_or_else(|| self.deezer.quality())
                    .as_str()
                    .to_string();
                let stream_id = uuid::Uuid::new_v4().to_string();
                let player_type = if is_radio_context(ctx_type) { 1 } else { 0 };
                *guard = Some(ActiveListen {
                    track_id: n.track_id,
                    duration_secs: n.duration_secs,
                    started_unix: crate::now_secs(),
                    stream_id: stream_id.clone(),
                    format,
                    context_id: ctx_id,
                    context_type: ctx_type,
                    player_type,
                    is_shuffle: n.is_shuffle,
                    repeat_mode: n.repeat_mode,
                    accumulated_secs: 0.0,
                    resumed_at: Some(now),
                    pause_count: 0,
                    seek_count: 0,
                    milestone_sent: false,
                    final_sent: false,
                });
                self.spawn_milestone_watcher(stream_id);
            }
            prev
        };

        let body = match (&outgoing_payload, next_id) {
            (Some((_, _, _, _, params)), n) => Some(build_log_listen_body(Some(params.clone()), n)),
            (None, Some(n)) => Some(build_log_listen_body(None, Some(n))),
            (None, None) => None,
        };

        if let Some(body) = body {
            let deezer = self.deezer.clone();
            let log_summary = outgoing_payload
                .as_ref()
                .map(|(tid, lt, ctx, fmt, _)| format!("prev={tid} lt={lt}s ctx={ctx} fmt={fmt}"))
                .unwrap_or_else(|| "prev=none".into());
            tokio::spawn(async move {
                match deezer.send_log_listen_raw(body).await {
                    Ok(()) => log::info!(
                        "[deezer-listen] reported ({log_summary}, next={next_id:?})"
                    ),
                    Err(e) => log::warn!(
                        "[deezer-listen] log.listen failed ({log_summary}, next={next_id:?}): {e}"
                    ),
                }
            });
        }
    }

    fn spawn_milestone_watcher(&self, stream_id: String) {
        let this = self.clone();
        tokio::spawn(async move {
            loop {
                let sleep_secs = {
                    let guard = this.active.lock().await;
                    let Some(cur) = guard.as_ref() else {
                        return;
                    };
                    if cur.stream_id != stream_id || cur.milestone_sent || cur.final_sent {
                        return;
                    }
                    let lt = cur.listened_secs(Instant::now());
                    if lt >= MILESTONE_SECS {
                        0
                    } else if cur.resumed_at.is_some() {
                        (MILESTONE_SECS - lt).max(1)
                    } else {
                        // Paused: re-check lightly until resumed or replaced.
                        5
                    }
                };
                if sleep_secs == 0 {
                    this.send_milestone_if_matching(&stream_id).await;
                    return;
                }
                tokio::time::sleep(Duration::from_secs(sleep_secs)).await;
            }
        });
    }

    async fn send_milestone_if_matching(&self, stream_id: &str) {
        let payload = {
            let mut guard = self.active.lock().await;
            let Some(cur) = guard.as_mut() else {
                return;
            };
            if cur.stream_id != stream_id || cur.milestone_sent || cur.final_sent {
                return;
            }
            let lt = cur.listened_secs(Instant::now());
            if lt < MILESTONE_SECS {
                return;
            }
            cur.milestone_sent = true;
            (
                cur.track_id,
                lt,
                cur.context_type,
                cur.format.clone(),
                cur.to_params_json(lt),
            )
        };
        let (tid, lt, ctx, fmt, params) = payload;
        let body = build_log_listen_body(Some(params), None);
        match self.deezer.send_log_listen_raw(body).await {
            Ok(()) => log::info!(
                "[deezer-listen] 30s milestone reported (track={tid} lt={lt}s ctx={ctx} fmt={fmt})"
            ),
            Err(e) => log::warn!("[deezer-listen] 30s milestone failed (track={tid}): {e}"),
        }
    }

    pub async fn on_pause(&self) {
        if let Some(cur) = self.active.lock().await.as_mut() {
            cur.pause(Instant::now());
        }
    }

    pub async fn on_resume(&self) {
        if let Some(cur) = self.active.lock().await.as_mut() {
            cur.resume(Instant::now());
        }
    }

    pub async fn on_seek(&self) {
        if let Some(cur) = self.active.lock().await.as_mut() {
            cur.seek_count = cur.seek_count.saturating_add(1);
        }
    }

    /// Called on natural end-of-track, gapless advance, explicit stop, or app exit.
    pub async fn finish_active(&self, reason: &'static str) {
        if !self.deezer.is_logged_in() {
            *self.active.lock().await = None;
            return;
        }
        let payload = {
            let mut guard = self.active.lock().await;
            let Some(mut cur) = guard.take() else {
                return;
            };
            if cur.final_sent {
                return;
            }
            cur.final_sent = true;
            let lt = cur.listened_secs(Instant::now());
            if lt == 0 && cur.milestone_sent {
                return;
            }
            (
                cur.track_id,
                lt,
                cur.context_type,
                cur.format.clone(),
                cur.to_params_json(lt),
            )
        };
        let (tid, lt, ctx, fmt, params) = payload;
        let body = build_log_listen_body(Some(params), None);
        match self.deezer.send_log_listen_raw(body).await {
            Ok(()) => log::info!(
                "[deezer-listen] final ({reason}) reported (track={tid} lt={lt}s ctx={ctx} fmt={fmt})"
            ),
            Err(e) => log::warn!("[deezer-listen] final ({reason}) failed (track={tid}): {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_log_listen_body_includes_params_and_next_media() {
        let body = build_log_listen_body(
            Some(json!({"media": {"id": "111", "type": "song", "format": "FLAC"}, "lt": 180})),
            Some(222),
        );
        assert_eq!(body["params"]["media"]["id"], "111");
        assert_eq!(body["params"]["lt"], 180);
        assert_eq!(body["next_media"]["media"]["id"], "222");
        assert_eq!(body["next_media"]["media"]["type"], "song");

        let start_only = build_log_listen_body(None, Some(333));
        assert!(start_only.get("params").is_none());
        assert_eq!(start_only["next_media"]["media"]["id"], "333");
    }

    #[test]
    fn active_listen_excludes_paused_time_and_tracks_stats() {
        let t0 = Instant::now();
        let mut s = ActiveListen {
            track_id: 99,
            duration_secs: 200,
            started_unix: 1_795_000_000,
            stream_id: "sid".into(),
            format: "FLAC".into(),
            context_id: "0".into(),
            context_type: "flow",
            player_type: 1,
            is_shuffle: true,
            repeat_mode: 1,
            accumulated_secs: 0.0,
            resumed_at: Some(t0),
            pause_count: 0,
            seek_count: 0,
            milestone_sent: false,
            final_sent: false,
        };
        s.pause(t0 + Duration::from_secs(12));
        s.seek_count += 2;
        s.resume(t0 + Duration::from_secs(50));
        assert_eq!(s.listened_secs(t0 + Duration::from_secs(60)), 22);
        let p = s.to_params_json(22);
        assert_eq!(p["lt"], 22);
        assert_eq!(p["type"], 1);
        assert_eq!(p["stat"]["pause"], 1);
        assert_eq!(p["stat"]["seek"], 2);
        assert_eq!(p["is_shuffle"], true);
        assert_eq!(p["repeat_type"], "repeat_all");
        assert_eq!(p["ctxt"]["t"], "flow");
    }
}

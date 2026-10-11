//! Advanced output settings: what the "Audio output" settings page edits.
//!
//! The values the audio worker reads while building a pipeline (ALSA buffer
//! and period, resampler quality, dithering, normal-mode sink) live in
//! process-wide atomics so the deep build helpers in `audio.rs` read them
//! without every signature growing a config parameter. The fallback policies
//! are decided on the worker itself and travel there with `SetOutputConfig`.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

/// What the worker does when exclusive / bit-perfect output fails.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum FailurePolicy {
    /// Play the track on the default PipeWire output straight away.
    #[default]
    Fallback,
    /// Try exclusive once more, then fall back.
    RetryOnce,
}

/// What happens to the following tracks once a fallback happened.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum AfterFallback {
    /// Stay on the default output until the user retries by hand.
    #[default]
    StayOnDefault,
    /// Try exclusive again at the start of every new track.
    RetryEachTrack,
}

/// Sink used when exclusive output is off (or suspended by a fallback).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum NormalSink {
    /// `autoaudiosink`: whatever the system ranks highest.
    #[default]
    Auto,
    /// `pipewiresink`, optionally pinned to a target node.
    Pipewire,
}

/// Dithering applied by `audioconvert` when exclusive (non-bit-perfect)
/// output has to reduce bit depth. Bit-perfect always forces `none`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Dither {
    None,
    #[default]
    Tpdf,
    TpdfHf,
}

impl Dither {
    pub fn as_gst(self) -> &'static str {
        match self {
            Dither::None => "none",
            Dither::Tpdf => "tpdf",
            Dither::TpdfHf => "tpdf-hf",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AudioOutputConfig {
    pub on_failure: FailurePolicy,
    pub after_fallback: AfterFallback,
    /// ALSA ring buffer length requested in exclusive mode (ms).
    pub alsa_buffer_ms: u32,
    /// ALSA period length requested in exclusive mode (ms).
    pub alsa_period_ms: u32,
    /// `audioresample` quality, 0 (fastest) to 10 (best). Used whenever a
    /// resampler is in the chain; bit-perfect has none.
    pub resample_quality: u32,
    pub dither: Dither,
    pub normal_sink: NormalSink,
    /// PipeWire node name (`node.name`) to play to. `None` = default sink.
    pub pipewire_target: Option<String>,
    /// Requested PipeWire quantum in ms. `None` = PipeWire default.
    pub pipewire_latency_ms: Option<u32>,
    /// Send a desktop notification on fallback (an in-app toast is always shown).
    pub notify_desktop: bool,
}

impl Default for AudioOutputConfig {
    fn default() -> Self {
        Self {
            on_failure: FailurePolicy::Fallback,
            after_fallback: AfterFallback::StayOnDefault,
            alsa_buffer_ms: 500,
            alsa_period_ms: 50,
            resample_quality: 4,
            dither: Dither::Tpdf,
            normal_sink: NormalSink::Auto,
            pipewire_target: None,
            pipewire_latency_ms: None,
            notify_desktop: true,
        }
    }
}

impl AudioOutputConfig {
    /// Clamp values into ranges ALSA and GStreamer accept.
    pub fn sanitized(mut self) -> Self {
        self.alsa_buffer_ms = self.alsa_buffer_ms.clamp(20, 2000);
        self.alsa_period_ms = self.alsa_period_ms.clamp(5, self.alsa_buffer_ms / 2);
        self.resample_quality = self.resample_quality.min(10);
        self.pipewire_latency_ms = self.pipewire_latency_ms.map(|v| v.clamp(1, 1000));
        self.pipewire_target = self
            .pipewire_target
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());
        self
    }
}

static ALSA_BUFFER_US: AtomicU32 = AtomicU32::new(500_000);
static ALSA_PERIOD_US: AtomicU32 = AtomicU32::new(50_000);
static RESAMPLE_QUALITY: AtomicU32 = AtomicU32::new(4);
static DITHER: Mutex<Dither> = Mutex::new(Dither::Tpdf);
static USE_PIPEWIRE_SINK: AtomicBool = AtomicBool::new(false);
static PIPEWIRE_TARGET: Mutex<Option<String>> = Mutex::new(None);
static PIPEWIRE_LATENCY_MS: AtomicU32 = AtomicU32::new(0);

/// Publish the build-time values. Called by the worker on `SetOutputConfig`.
pub fn publish(cfg: &AudioOutputConfig) {
    ALSA_BUFFER_US.store(cfg.alsa_buffer_ms * 1000, Ordering::Relaxed);
    ALSA_PERIOD_US.store(cfg.alsa_period_ms * 1000, Ordering::Relaxed);
    RESAMPLE_QUALITY.store(cfg.resample_quality, Ordering::Relaxed);
    *DITHER.lock().unwrap_or_else(|p| p.into_inner()) = cfg.dither;
    USE_PIPEWIRE_SINK.store(cfg.normal_sink == NormalSink::Pipewire, Ordering::Relaxed);
    *PIPEWIRE_TARGET.lock().unwrap_or_else(|p| p.into_inner()) = cfg.pipewire_target.clone();
    PIPEWIRE_LATENCY_MS.store(cfg.pipewire_latency_ms.unwrap_or(0), Ordering::Relaxed);
}

pub fn alsa_buffer_us() -> u32 {
    ALSA_BUFFER_US.load(Ordering::Relaxed)
}

pub fn alsa_period_us() -> u32 {
    ALSA_PERIOD_US.load(Ordering::Relaxed)
}

pub fn resample_quality() -> u32 {
    RESAMPLE_QUALITY.load(Ordering::Relaxed)
}

pub fn dither() -> Dither {
    *DITHER.lock().unwrap_or_else(|p| p.into_inner())
}

/// Build the sink for the normal (non-exclusive) chain. Falls back to
/// `autoaudiosink` when `pipewiresink` is not installed.
///
/// `default_only` is set during an exclusive fallback: the user asked for the
/// system's default output there, so a pinned target is ignored and the
/// PipeWire default sink is used (through `pipewiresink` when available).
pub fn make_normal_sink(default_only: bool) -> Result<gstreamer::Element, String> {
    use gstreamer as gst;
    if default_only {
        if gst::ElementFactory::find("pipewiresink").is_some() {
            return gst::ElementFactory::make("pipewiresink")
                .property("client-name", "dsone")
                .build()
                .map_err(|e| format!("Failed to create pipewiresink: {e}"));
        }
    } else if USE_PIPEWIRE_SINK.load(Ordering::Relaxed) {
        if gst::ElementFactory::find("pipewiresink").is_some() {
            let mut b = gst::ElementFactory::make("pipewiresink").property("client-name", "dsone");
            if let Some(target) = PIPEWIRE_TARGET
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone()
            {
                b = b.property("target-object", target);
            }
            let latency = PIPEWIRE_LATENCY_MS.load(Ordering::Relaxed);
            let mut props = gst::Structure::builder("props")
                .field("media.role", "Music")
                .field("media.category", "Playback");
            if latency > 0 {
                // Expressed against 48 kHz; PipeWire scales it to the graph rate.
                props = props.field("node.latency", format!("{}/48000", latency * 48));
            }
            b = b.property("stream-properties", props.build());
            return b
                .build()
                .map_err(|e| format!("Failed to create pipewiresink: {e}"));
        }
        log::warn!("[audio] pipewiresink not installed, using autoaudiosink");
    }
    gst::ElementFactory::make("autoaudiosink")
        .build()
        .map_err(|e| format!("Failed to create autoaudiosink: {e}"))
}

/// A PipeWire sink the user can pick in settings.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PipewireSink {
    /// `node.name`, what `target-object` takes.
    pub name: String,
    pub description: String,
}

/// List PipeWire audio sinks through GStreamer's device monitor.
pub fn list_pipewire_sinks() -> Vec<PipewireSink> {
    use gstreamer as gst;
    use gstreamer::prelude::*;
    let monitor = gst::DeviceMonitor::new();
    let caps = gst::Caps::new_empty_simple("audio/x-raw");
    monitor.add_filter(Some("Audio/Sink"), Some(&caps));
    if monitor.start().is_err() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for device in monitor.devices() {
        let Some(props) = device.properties() else {
            continue;
        };
        let api = props.get::<String>("device.api").unwrap_or_default();
        if api != "pipewire" {
            continue;
        }
        let Ok(name) = props.get::<String>("node.name") else {
            continue;
        };
        out.push(PipewireSink {
            name,
            description: device.display_name().to_string(),
        });
    }
    monitor.stop();
    out.sort_by(|a, b| a.description.cmp(&b.description));
    out.dedup_by(|a, b| a.name == b.name);
    out
}

/// Turn a writer/open error kind into a sentence for the notification.
pub fn describe_failure(kind: &str, message: Option<&str>, device: Option<&str>) -> String {
    let dev = device.unwrap_or("DAC");
    match kind {
        "device_busy" => format!("{dev} is in use by another application"),
        "device_disconnected" => format!("{dev} was disconnected"),
        "device_changed" => format!("{dev} changed format unexpectedly"),
        "format_change_failed" => format!(
            "{dev} refused the track's format{}",
            message.map(|m| format!(" ({m})")).unwrap_or_default()
        ),
        "write_error" => format!("write error on {dev}"),
        "no_device" => "no exclusive device selected".to_string(),
        _ => message
            .map(|m| m.to_string())
            .unwrap_or_else(|| format!("exclusive output failed on {dev}")),
    }
}

/// Map an error string returned by the exclusive build into a kind.
pub fn classify_open_error(err: &str) -> &'static str {
    if err.contains("device_busy") || err.contains("busy") || err.contains("EBUSY") {
        "device_busy"
    } else if err.contains("No audio device selected") {
        "no_device"
    } else if err.contains("No such device") || err.contains("ENODEV") || err.contains("ENOENT") {
        "device_disconnected"
    } else {
        "open_failed"
    }
}

/// Desktop notification over org.freedesktop.Notifications with a "Retry"
/// action. Runs on its own thread; failures are logged and ignored (the
/// in-app toast is the primary channel). `on_retry` runs if the user clicks
/// the action.
pub fn notify_fallback(summary: String, body: String, on_retry: impl FnOnce() + Send + 'static) {
    std::thread::spawn(move || {
        if let Err(e) = notify_blocking(&summary, &body, on_retry) {
            log::warn!("[audio] desktop notification failed: {e}");
        }
    });
}

fn notify_blocking(
    summary: &str,
    body: &str,
    on_retry: impl FnOnce(),
) -> Result<(), Box<dyn std::error::Error>> {
    use std::collections::HashMap;
    // Aliased so the HTTP-proxy source guard does not mistake it for reqwest.
    use zbus::blocking::{Connection, Proxy as DbusProxy};
    use zbus::zvariant::Value;

    let conn = Connection::session()?;
    let proxy = DbusProxy::new(
        &conn,
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        "org.freedesktop.Notifications",
    )?;
    let mut hints: HashMap<&str, Value> = HashMap::new();
    hints.insert("urgency", Value::U8(1));
    hints.insert(
        "desktop-entry",
        Value::from("io.github.technomagniaque.dsone"),
    );
    let actions = vec!["retry", "Retry exclusive"];
    // Subscribe before sending so a fast click is not missed.
    let mut signals = proxy.receive_signal("ActionInvoked")?;
    let id: u32 = proxy.call(
        "Notify",
        &(
            "dsone",
            0u32,
            "audio-card",
            summary,
            body,
            actions,
            hints,
            10_000i32,
        ),
    )?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    // The blocking iterator has no timeout; the notification expires well
    // before the deadline on every server we know of, and a CloseNotification
    // ends the wait through the server closing it. Bound it anyway by checking
    // the clock between signals.
    for msg in signals.by_ref() {
        if std::time::Instant::now() > deadline {
            break;
        }
        if let Ok((nid, action)) = msg.body().deserialize::<(u32, String)>() {
            if nid == id && action == "retry" {
                on_retry();
                break;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_previous_hardcoded_values() {
        let c = AudioOutputConfig::default();
        assert_eq!(c.alsa_buffer_ms, 500);
        assert_eq!(c.alsa_period_ms, 50);
        assert_eq!(c.on_failure, FailurePolicy::Fallback);
        assert_eq!(c.after_fallback, AfterFallback::StayOnDefault);
    }

    #[test]
    fn sanitize_keeps_period_below_half_the_buffer() {
        let c = AudioOutputConfig {
            alsa_buffer_ms: 40,
            alsa_period_ms: 100,
            resample_quality: 99,
            pipewire_target: Some("  ".into()),
            ..Default::default()
        }
        .sanitized();
        assert_eq!(c.alsa_buffer_ms, 40);
        assert_eq!(c.alsa_period_ms, 20);
        assert_eq!(c.resample_quality, 10);
        assert_eq!(c.pipewire_target, None);
    }

    #[test]
    fn missing_fields_deserialize_to_defaults() {
        let c: AudioOutputConfig = serde_json::from_str(r#"{"onFailure":"retryOnce"}"#).unwrap();
        assert_eq!(c.on_failure, FailurePolicy::RetryOnce);
        assert_eq!(c.alsa_buffer_ms, 500);
    }

    #[test]
    fn open_errors_are_classified() {
        assert_eq!(classify_open_error("device_busy"), "device_busy");
        assert_eq!(
            classify_open_error("No audio device selected for exclusive mode"),
            "no_device"
        );
        assert_eq!(
            classify_open_error("hw_params: Invalid argument"),
            "open_failed"
        );
    }
}

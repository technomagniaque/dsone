import { invoke } from "@tauri-apps/api/core";

export type FailurePolicy = "fallback" | "retryOnce";
export type AfterFallback = "stayOnDefault" | "retryEachTrack";
export type NormalSink = "auto" | "pipewire";
export type Dither = "none" | "tpdf" | "tpdfHf";

/** Mirrors `AudioOutputConfig` in src-tauri/src/audio_output.rs. */
export interface AudioOutputConfig {
  onFailure: FailurePolicy;
  afterFallback: AfterFallback;
  alsaBufferMs: number;
  alsaPeriodMs: number;
  resampleQuality: number;
  dither: Dither;
  normalSink: NormalSink;
  pipewireTarget: string | null;
  pipewireLatencyMs: number | null;
  notifyDesktop: boolean;
}

export interface PipewireSink {
  name: string;
  description: string;
}

export const DEFAULT_AUDIO_OUTPUT: AudioOutputConfig = {
  onFailure: "fallback",
  afterFallback: "stayOnDefault",
  alsaBufferMs: 500,
  alsaPeriodMs: 50,
  resampleQuality: 4,
  dither: "tpdf",
  normalSink: "auto",
  pipewireTarget: null,
  pipewireLatencyMs: null,
  notifyDesktop: true,
};

/** ALSA buffer / period presets offered next to the free inputs. */
export const BUFFER_PRESETS = [
  { id: "low", label: "Low latency", bufferMs: 100, periodMs: 10 },
  { id: "balanced", label: "Balanced", bufferMs: 500, periodMs: 50 },
  { id: "safe", label: "Safe", bufferMs: 1000, periodMs: 100 },
] as const;

export function presetFor(cfg: AudioOutputConfig): string | null {
  const p = BUFFER_PRESETS.find(
    (x) => x.bufferMs === cfg.alsaBufferMs && x.periodMs === cfg.alsaPeriodMs,
  );
  return p ? p.id : null;
}

export async function loadAudioOutputConfig(): Promise<AudioOutputConfig> {
  const cfg = await invoke<AudioOutputConfig>("get_audio_output_config");
  return { ...DEFAULT_AUDIO_OUTPUT, ...cfg };
}

/** Saves and returns the config as the backend sanitized it. */
export async function saveAudioOutputConfig(
  cfg: AudioOutputConfig,
): Promise<AudioOutputConfig> {
  return invoke<AudioOutputConfig>("set_audio_output_config", { config: cfg });
}

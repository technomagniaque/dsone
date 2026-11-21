import { DEFAULT_AUDIO_OUTPUT, type AudioOutputConfig } from "./audioOutput";

export const AUDIO_OUTPUT_PROFILES_KEY = "sone.audio-output-profiles.v1";

export interface AudioOutputProfile {
  id: string;
  name: string;
  builtin?: boolean;
  exclusiveMode: boolean;
  bitPerfect: boolean;
  exclusiveDevice: string | null;
  config: AudioOutputConfig;
}

export function createDefaultProfiles(
  preferredAlsaDevice: string | null = null,
  baseConfig: AudioOutputConfig = DEFAULT_AUDIO_OUTPUT,
): AudioOutputProfile[] {
  return [
    {
      id: "builtin-alsa-bit-perfect",
      name: "ALSA Bit-Perfect",
      builtin: true,
      exclusiveMode: true,
      bitPerfect: true,
      exclusiveDevice: preferredAlsaDevice,
      config: { ...baseConfig },
    },
    {
      id: "builtin-pipewire-desktop",
      name: "PipeWire Desktop",
      builtin: true,
      exclusiveMode: false,
      bitPerfect: false,
      exclusiveDevice: preferredAlsaDevice,
      config: { ...baseConfig, normalSink: "pipewire" },
    },
  ];
}

export function loadAudioOutputProfiles(
  preferredAlsaDevice: string | null = null,
  baseConfig: AudioOutputConfig = DEFAULT_AUDIO_OUTPUT,
): AudioOutputProfile[] {
  try {
    const raw = localStorage.getItem(AUDIO_OUTPUT_PROFILES_KEY);
    if (!raw) return createDefaultProfiles(preferredAlsaDevice, baseConfig);
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed) || parsed.length === 0) {
      return createDefaultProfiles(preferredAlsaDevice, baseConfig);
    }
    return parsed
      .filter(
        (p): p is AudioOutputProfile =>
          !!p && typeof p.id === "string" && typeof p.name === "string",
      )
      .map((p) => ({
        ...p,
        config: { ...DEFAULT_AUDIO_OUTPUT, ...(p.config ?? {}) },
      }));
  } catch {
    return createDefaultProfiles(preferredAlsaDevice, baseConfig);
  }
}

export function saveAudioOutputProfiles(profiles: AudioOutputProfile[]): void {
  try {
    localStorage.setItem(AUDIO_OUTPUT_PROFILES_KEY, JSON.stringify(profiles));
  } catch {
    // Ignore storage quota errors in restricted environments.
  }
}

export function upsertAudioOutputProfile(
  profiles: AudioOutputProfile[],
  profile: AudioOutputProfile,
): AudioOutputProfile[] {
  const exists = profiles.some((p) => p.id === profile.id);
  if (exists) {
    return profiles.map((p) => (p.id === profile.id ? { ...profile } : p));
  }
  return [...profiles, { ...profile }];
}

export function removeAudioOutputProfile(
  profiles: AudioOutputProfile[],
  id: string,
): AudioOutputProfile[] {
  return profiles.filter((p) => p.id !== id || p.builtin);
}

/** Find the profile that best matches the active output state. */
export function findActiveProfileId(
  profiles: AudioOutputProfile[],
  state: {
    exclusiveMode: boolean;
    bitPerfect: boolean;
    exclusiveDevice: string | null;
    pipewireTarget: string | null;
  },
): string | null {
  const exact = profiles.find(
    (p) =>
      p.exclusiveMode === state.exclusiveMode &&
      p.bitPerfect === state.bitPerfect &&
      (state.exclusiveMode
        ? !p.exclusiveDevice || p.exclusiveDevice === state.exclusiveDevice
        : (p.config.pipewireTarget ?? null) === (state.pipewireTarget ?? null)),
  );
  if (exact) return exact.id;
  const byMode = profiles.find(
    (p) =>
      p.exclusiveMode === state.exclusiveMode &&
      p.bitPerfect === state.bitPerfect,
  );
  return byMode?.id ?? null;
}

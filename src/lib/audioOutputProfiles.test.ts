import { describe, expect, it } from "vitest";
import { DEFAULT_AUDIO_OUTPUT } from "./audioOutput";
import {
  createDefaultProfiles,
  findActiveProfileId,
  removeAudioOutputProfile,
  upsertAudioOutputProfile,
} from "./audioOutputProfiles";

describe("audioOutputProfiles", () => {
  it("provides ALSA Bit-Perfect and PipeWire Desktop default profiles", () => {
    const defaults = createDefaultProfiles("hw:0,0");
    expect(defaults.map((p) => p.name)).toEqual([
      "ALSA Bit-Perfect",
      "PipeWire Desktop",
    ]);
    expect(defaults[0].exclusiveMode).toBe(true);
    expect(defaults[0].bitPerfect).toBe(true);
    expect(defaults[0].exclusiveDevice).toBe("hw:0,0");
    expect(defaults[1].exclusiveMode).toBe(false);
    expect(defaults[1].bitPerfect).toBe(false);
  });

  it("matches active profile and supports immutable custom profiles", () => {
    const base = createDefaultProfiles("hw:0,0");
    const custom = {
      id: "custom-rme-pw",
      name: "RME PipeWire Pro",
      exclusiveMode: false,
      bitPerfect: false,
      exclusiveDevice: null,
      config: {
        ...DEFAULT_AUDIO_OUTPUT,
        normalSink: "pipewire" as const,
        pipewireTarget: "alsa_output.pci-0000_05_00.0.pro-output-0",
      },
    };
    const next = upsertAudioOutputProfile(base, custom);
    expect(next).toHaveLength(3);
    expect(base).toHaveLength(2); // immutable

    expect(
      findActiveProfileId(next, {
        exclusiveMode: false,
        bitPerfect: false,
        exclusiveDevice: "hw:0,0",
        pipewireTarget: "alsa_output.pci-0000_05_00.0.pro-output-0",
      }),
    ).toBe("custom-rme-pw");

    // Built-in profiles cannot be removed; custom profiles can.
    expect(removeAudioOutputProfile(next, base[0].id)).toHaveLength(3);
    expect(removeAudioOutputProfile(next, "custom-rme-pw")).toHaveLength(2);
  });
});

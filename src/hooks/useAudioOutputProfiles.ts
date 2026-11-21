import { useCallback, useEffect, useState } from "react";
import { useAtom } from "jotai";
import { invoke } from "@tauri-apps/api/core";
import {
  bitPerfectAtom,
  exclusiveDeviceAtom,
  exclusiveModeAtom,
} from "../atoms/playback";
import { usePlaybackActions } from "./usePlaybackActions";
import { useToast } from "../contexts/ToastContext";
import {
  DEFAULT_AUDIO_OUTPUT,
  loadAudioOutputConfig,
  saveAudioOutputConfig,
  type AudioOutputConfig,
} from "../lib/audioOutput";
import {
  findActiveProfileId,
  loadAudioOutputProfiles,
  removeAudioOutputProfile,
  saveAudioOutputProfiles,
  upsertAudioOutputProfile,
  type AudioOutputProfile,
} from "../lib/audioOutputProfiles";

export function useAudioOutputProfiles(externalConfig?: AudioOutputConfig) {
  const [exclusiveMode, setExclusiveMode] = useAtom(exclusiveModeAtom);
  const [bitPerfect] = useAtom(bitPerfectAtom);
  const [exclusiveDevice, setExclusiveDevice] = useAtom(exclusiveDeviceAtom);
  const { setBitPerfect } = usePlaybackActions();
  const { showToast } = useToast();

  const [internalCfg, setInternalCfg] =
    useState<AudioOutputConfig>(DEFAULT_AUDIO_OUTPUT);
  const activeCfg = externalConfig ?? internalCfg;

  const [profiles, setProfiles] = useState<AudioOutputProfile[]>(() =>
    loadAudioOutputProfiles(exclusiveDevice, activeCfg),
  );
  const [switchingId, setSwitchingId] = useState<string | null>(null);

  useEffect(() => {
    if (externalConfig) return;
    loadAudioOutputConfig()
      .then(setInternalCfg)
      .catch(() => {});
  }, [externalConfig]);

  const activeProfileId = findActiveProfileId(profiles, {
    exclusiveMode,
    bitPerfect,
    exclusiveDevice,
    pipewireTarget: activeCfg.pipewireTarget,
  });

  const applyProfile = useCallback(
    async (
      profile: AudioOutputProfile,
      onConfigApplied?: (cfg: AudioOutputConfig) => void,
    ) => {
      setSwitchingId(profile.id);
      try {
        let deviceToUse = profile.exclusiveDevice ?? exclusiveDevice;
        if (profile.exclusiveMode && !deviceToUse) {
          const devices = await invoke<{ id: string; name: string }[]>(
            "list_audio_devices",
          ).catch(() => []);
          if (devices.length > 0) deviceToUse = devices[0].id;
        }

        const savedCfg = await saveAudioOutputConfig(profile.config);
        setInternalCfg(savedCfg);
        onConfigApplied?.(savedCfg);

        setExclusiveMode(profile.exclusiveMode);
        await invoke("set_exclusive_mode", {
          enabled: profile.exclusiveMode,
        }).catch(() => {});

        if (profile.exclusiveMode && deviceToUse) {
          setExclusiveDevice(deviceToUse);
          await invoke("set_exclusive_device", { device: deviceToUse }).catch(
            () => {},
          );
        }

        setBitPerfect(profile.exclusiveMode ? profile.bitPerfect : false);

        showToast(
          `Output profile: ${profile.name} — takes effect next track`,
          "info",
        );
      } catch {
        showToast(`Failed to apply output profile "${profile.name}"`, "error");
      } finally {
        setSwitchingId(null);
      }
    },
    [
      exclusiveDevice,
      setExclusiveMode,
      setExclusiveDevice,
      setBitPerfect,
      showToast,
    ],
  );

  const saveCurrentAsProfile = useCallback(
    (name: string, currentConfig: AudioOutputConfig) => {
      const trimmed = name.trim();
      if (!trimmed) return;
      const nextProfile: AudioOutputProfile = {
        id: `custom-${Date.now()}`,
        name: trimmed,
        exclusiveMode,
        bitPerfect,
        exclusiveDevice,
        config: { ...currentConfig },
      };
      setProfiles((prev) => {
        const next = upsertAudioOutputProfile(prev, nextProfile);
        saveAudioOutputProfiles(next);
        return next;
      });
      showToast(`Saved output profile "${trimmed}"`);
    },
    [exclusiveMode, bitPerfect, exclusiveDevice, showToast],
  );

  const updateProfileWithCurrent = useCallback(
    (profile: AudioOutputProfile, currentConfig: AudioOutputConfig) => {
      const updated: AudioOutputProfile = {
        ...profile,
        exclusiveMode,
        bitPerfect,
        exclusiveDevice,
        config: { ...currentConfig },
      };
      setProfiles((prev) => {
        const next = upsertAudioOutputProfile(prev, updated);
        saveAudioOutputProfiles(next);
        return next;
      });
      showToast(`Updated profile "${profile.name}" with current settings`);
    },
    [exclusiveMode, bitPerfect, exclusiveDevice, showToast],
  );

  const deleteProfile = useCallback(
    (id: string) => {
      setProfiles((prev) => {
        const next = removeAudioOutputProfile(prev, id);
        saveAudioOutputProfiles(next);
        return next;
      });
    },
    [],
  );

  return {
    profiles,
    activeProfileId,
    switchingId,
    applyProfile,
    saveCurrentAsProfile,
    updateProfileWithCurrent,
    deleteProfile,
  };
}

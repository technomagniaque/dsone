import { useEffect, useState, type ReactNode } from "react";
import { useAtom, useAtomValue } from "jotai";
import { invoke } from "@tauri-apps/api/core";
import {
  bitPerfectAtom,
  exclusiveDeviceAtom,
  exclusiveModeAtom,
  signalPathAtom,
} from "../../atoms/playback";
import { usePlaybackActions } from "../../hooks/usePlaybackActions";
import {
  BUFFER_PRESETS,
  DEFAULT_AUDIO_OUTPUT,
  loadAudioOutputConfig,
  presetFor,
  saveAudioOutputConfig,
  type AudioOutputConfig,
  type PipewireSink,
} from "../../lib/audioOutput";
import Toggle from "../Toggle";
import SettingRow from "./SettingRow";

const GROUP_CLASS =
  "rounded-[14px] bg-th-surface border border-th-border-subtle overflow-hidden divide-y divide-th-border-subtle";
const GROUP_STYLE = { boxShadow: "inset 0 2px 8px rgba(0,0,0,.32)" };
const SELECT_CLASS =
  "bg-th-inset border border-th-border-subtle rounded-md px-2.5 py-1.5 text-[12.5px] text-th-text-primary max-w-[230px] disabled:opacity-50";
const NUMBER_CLASS =
  "w-[72px] bg-th-inset border border-th-border-subtle rounded-md px-2 py-1 text-[12.5px] text-th-text-primary text-right";

function SectionLabel({ children }: { children: ReactNode }) {
  return (
    <p className="text-[10.5px] font-bold tracking-[1.4px] uppercase text-th-text-faint mt-6 mb-1 first:mt-0">
      {children}
    </p>
  );
}

function Choice<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: { id: T; label: string }[];
  onChange: (v: T) => void;
}) {
  return (
    <div className="flex rounded-md border border-th-border-subtle overflow-hidden shrink-0">
      {options.map((o) => (
        <button
          key={o.id}
          onClick={() => onChange(o.id)}
          className={`px-2.5 py-1 text-[12px] transition-colors ${
            value === o.id
              ? "bg-th-accent/15 text-th-accent font-semibold"
              : "text-th-text-secondary hover:bg-th-inset/60"
          }`}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export default function AudioOutputTab() {
  const [exclusiveMode, setExclusiveMode] = useAtom(exclusiveModeAtom);
  const bitPerfect = useAtomValue(bitPerfectAtom);
  const [exclusiveDevice, setExclusiveDevice] = useAtom(exclusiveDeviceAtom);
  const signalPath = useAtomValue(signalPathAtom);
  const { setBitPerfect } = usePlaybackActions();

  const [cfg, setCfg] = useState<AudioOutputConfig>(DEFAULT_AUDIO_OUTPUT);
  const [devices, setDevices] = useState<{ id: string; name: string }[]>([]);
  const [sinks, setSinks] = useState<PipewireSink[]>([]);
  const [retrying, setRetrying] = useState(false);

  useEffect(() => {
    loadAudioOutputConfig()
      .then(setCfg)
      .catch(() => {});
    invoke<{ id: string; name: string }[]>("list_audio_devices")
      .then(setDevices)
      .catch(() => {});
    invoke<PipewireSink[]>("list_pipewire_sinks")
      .then(setSinks)
      .catch(() => {});
  }, []);

  const update = (patch: Partial<AudioOutputConfig>) => {
    const next = { ...cfg, ...patch };
    setCfg(next);
    saveAudioOutputConfig(next)
      .then(setCfg)
      .catch(() => {});
  };

  const toggleExclusive = () => {
    const next = !exclusiveMode;
    setExclusiveMode(next);
    if (!next && bitPerfect) setBitPerfect(false);
    invoke("set_exclusive_mode", { enabled: next }).catch(() => {});
  };

  const pickDevice = (id: string) => {
    setExclusiveDevice(id);
    invoke("set_exclusive_device", { device: id }).catch(() => {});
  };

  const retryNow = async () => {
    setRetrying(true);
    try {
      await invoke("retry_exclusive_output");
    } catch {
      // The backend falls back again and reports it through audio-fallback.
    } finally {
      setRetrying(false);
    }
  };

  const fallbackReason = signalPath?.fallbackReason ?? null;
  const preset = presetFor(cfg);

  return (
    <div>
      {fallbackReason && (
        <div className="mb-5 rounded-[12px] border border-amber-500/40 bg-amber-500/10 px-4 py-3 flex items-center gap-3">
          <div className="flex-1 min-w-0">
            <p className="text-[13px] font-semibold text-th-text-primary">
              Playing on the default output
            </p>
            <p className="text-[11.5px] text-th-text-muted mt-0.5">
              Exclusive output failed: {fallbackReason}
            </p>
          </div>
          <button
            onClick={retryNow}
            disabled={retrying}
            className="px-3 py-1.5 rounded-md bg-th-accent text-black text-[12.5px] font-semibold disabled:opacity-60"
          >
            {retrying ? "Retrying…" : "Retry"}
          </button>
        </div>
      )}

      <SectionLabel>Exclusive output</SectionLabel>
      <div className={GROUP_CLASS} style={GROUP_STYLE}>
        <SettingRow
          title="Exclusive output"
          subtitle="Open the DAC directly through ALSA, bypassing the system mixer"
        >
          <button onClick={toggleExclusive}>
            <Toggle on={exclusiveMode} />
          </button>
        </SettingRow>
        <SettingRow
          title="Device"
          subtitle="ALSA device used for exclusive and bit-perfect output"
          disabled={!exclusiveMode}
        >
          <select
            className={SELECT_CLASS}
            disabled={!exclusiveMode || devices.length === 0}
            value={exclusiveDevice ?? ""}
            onChange={(e) => pickDevice(e.target.value)}
          >
            {devices.length === 0 && <option value="">No device found</option>}
            {devices.map((d) => (
              <option key={d.id} value={d.id}>
                {d.name}
              </option>
            ))}
          </select>
        </SettingRow>
        <SettingRow
          title="Bit-perfect"
          subtitle="No resampling, no dithering, no software volume or normalization"
          disabled={!exclusiveMode}
        >
          <button
            disabled={!exclusiveMode}
            className="disabled:cursor-not-allowed"
            onClick={() => setBitPerfect(!bitPerfect)}
          >
            <Toggle on={bitPerfect} />
          </button>
        </SettingRow>
        <SettingRow
          title="ALSA buffer"
          subtitle="Larger buffers survive system load better; smaller ones react faster"
          disabled={!exclusiveMode}
        >
          <Choice
            value={preset ?? "custom"}
            options={[
              ...BUFFER_PRESETS.map((p) => ({ id: p.id, label: p.label })),
              { id: "custom", label: "Custom" },
            ]}
            onChange={(id) => {
              const p = BUFFER_PRESETS.find((x) => x.id === id);
              if (p)
                update({ alsaBufferMs: p.bufferMs, alsaPeriodMs: p.periodMs });
            }}
          />
        </SettingRow>
        <SettingRow
          title="Buffer / period (ms)"
          subtitle="Applied when the device is next opened"
          disabled={!exclusiveMode}
        >
          <div className="flex items-center gap-1.5">
            <input
              type="number"
              min={20}
              max={2000}
              className={NUMBER_CLASS}
              value={cfg.alsaBufferMs}
              onChange={(e) =>
                setCfg({ ...cfg, alsaBufferMs: Number(e.target.value) })
              }
              onBlur={() => update({})}
              aria-label="Buffer length in milliseconds"
            />
            <span className="text-th-text-faint text-[12px]">/</span>
            <input
              type="number"
              min={5}
              max={1000}
              className={NUMBER_CLASS}
              value={cfg.alsaPeriodMs}
              onChange={(e) =>
                setCfg({ ...cfg, alsaPeriodMs: Number(e.target.value) })
              }
              onBlur={() => update({})}
              aria-label="Period length in milliseconds"
            />
          </div>
        </SettingRow>
        <SettingRow
          title="Resampler quality"
          subtitle={
            bitPerfect
              ? "Unused in bit-perfect mode (no resampler)"
              : `${cfg.resampleQuality} / 10 — used when the DAC lacks the track's rate`
          }
          disabled={bitPerfect}
        >
          <input
            type="range"
            min={0}
            max={10}
            step={1}
            disabled={bitPerfect}
            value={cfg.resampleQuality}
            onChange={(e) =>
              setCfg({ ...cfg, resampleQuality: Number(e.target.value) })
            }
            onMouseUp={() => update({})}
            onKeyUp={() => update({})}
            aria-label="Resampler quality"
          />
        </SettingRow>
        <SettingRow
          title="Dithering"
          subtitle={
            bitPerfect
              ? "Always off in bit-perfect mode"
              : "Applied when the bit depth has to be reduced"
          }
          disabled={bitPerfect}
        >
          <select
            className={SELECT_CLASS}
            disabled={bitPerfect}
            value={cfg.dither}
            onChange={(e) =>
              update({ dither: e.target.value as AudioOutputConfig["dither"] })
            }
          >
            <option value="none">None</option>
            <option value="tpdf">TPDF</option>
            <option value="tpdfHf">TPDF high-frequency</option>
          </select>
        </SettingRow>
      </div>

      <SectionLabel>If exclusive output fails</SectionLabel>
      <div className={GROUP_CLASS} style={GROUP_STYLE}>
        <SettingRow
          title="On failure"
          subtitle="The track always keeps playing, on the default PipeWire output if needed"
        >
          <Choice
            value={cfg.onFailure}
            options={[
              { id: "fallback", label: "Fall back now" },
              { id: "retryOnce", label: "Retry once first" },
            ]}
            onChange={(v) => update({ onFailure: v })}
          />
        </SettingRow>
        <SettingRow
          title="After a fallback"
          subtitle={
            cfg.afterFallback === "stayOnDefault"
              ? "Stay on the default output until you retry"
              : "Try exclusive again at the start of each new track"
          }
        >
          <Choice
            value={cfg.afterFallback}
            options={[
              { id: "stayOnDefault", label: "Stay on default" },
              { id: "retryEachTrack", label: "Retry each track" },
            ]}
            onChange={(v) => update({ afterFallback: v })}
          />
        </SettingRow>
        <SettingRow
          title="Desktop notification"
          subtitle="Also notify outside the app, with a Retry button"
        >
          <button onClick={() => update({ notifyDesktop: !cfg.notifyDesktop })}>
            <Toggle on={cfg.notifyDesktop} />
          </button>
        </SettingRow>
      </div>

      <SectionLabel>Normal output</SectionLabel>
      <div className={GROUP_CLASS} style={GROUP_STYLE}>
        <SettingRow title="Output" subtitle="Used when exclusive output is off">
          <Choice
            value={cfg.normalSink}
            options={[
              { id: "auto", label: "Automatic" },
              { id: "pipewire", label: "PipeWire" },
            ]}
            onChange={(v) => update({ normalSink: v })}
          />
        </SettingRow>
        <SettingRow
          title="PipeWire device"
          subtitle="Leave on default to follow the system's output"
          disabled={cfg.normalSink !== "pipewire"}
        >
          <select
            className={SELECT_CLASS}
            disabled={cfg.normalSink !== "pipewire"}
            value={cfg.pipewireTarget ?? ""}
            onChange={(e) => update({ pipewireTarget: e.target.value || null })}
          >
            <option value="">System default</option>
            {sinks.map((s) => (
              <option key={s.name} value={s.name}>
                {s.description}
              </option>
            ))}
          </select>
        </SettingRow>
        <SettingRow
          title="PipeWire latency (ms)"
          subtitle="Empty uses PipeWire's default quantum"
          disabled={cfg.normalSink !== "pipewire"}
        >
          <input
            type="number"
            min={1}
            max={1000}
            className={NUMBER_CLASS}
            disabled={cfg.normalSink !== "pipewire"}
            value={cfg.pipewireLatencyMs ?? ""}
            onChange={(e) =>
              setCfg({
                ...cfg,
                pipewireLatencyMs:
                  e.target.value === "" ? null : Number(e.target.value),
              })
            }
            onBlur={() => update({})}
            aria-label="PipeWire latency in milliseconds"
          />
        </SettingRow>
      </div>
      <p className="text-[11px] text-th-text-muted mt-2.5">
        Output changes apply from the next track. The fallback always uses the
        system's default output.
      </p>
    </div>
  );
}

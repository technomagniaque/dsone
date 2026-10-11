import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { renderHook, act, cleanup } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { invoke } from "@tauri-apps/api/core";
import type { PropsWithChildren } from "react";
import { usePlaybackActions } from "./usePlaybackActions";
import { ToastProvider } from "../contexts/ToastContext";
import {
  currentTrackAtom,
  isPlayingAtom,
  useTrackGainAtom,
} from "../atoms/playback";
import type { Track } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn((cmd: string) => {
    if (cmd === "play_tidal_track" || cmd === "play_deezer_track")
      return Promise.resolve({ url: "x", trackId: 1 });
    if (cmd === "is_track_finished") return Promise.resolve(true);
    return Promise.resolve(undefined);
  }),
}));

afterEach(cleanup);

const track = (over: Partial<Track> = {}): Track =>
  ({ id: 42, title: "T", duration: 100, ...over }) as Track;

function setup() {
  const store = createStore();
  const wrapper = ({ children }: PropsWithChildren) => (
    <Provider store={store}>
      <ToastProvider>{children}</ToastProvider>
    </Provider>
  );
  const { result } = renderHook(() => usePlaybackActions(), { wrapper });
  return { store, result };
}

const calls = (cmd: string) =>
  vi.mocked(invoke).mock.calls.filter((c) => c[0] === cmd);

describe("playback dispatches on the track's provider", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.mocked(invoke).mockClear();
  });

  it("plays a Deezer track with play_deezer_track, never the TIDAL command", async () => {
    const { store, result } = setup();
    store.set(useTrackGainAtom, true);
    await act(async () => {
      await result.current.playTrack(track({ provider: "deezer" }));
    });
    expect(calls("play_deezer_track")).toEqual([
      ["play_deezer_track", { trackId: 42, useTrackGain: true }],
    ]);
    expect(calls("play_tidal_track")).toHaveLength(0);
    expect(store.get(isPlayingAtom)).toBe(true);
    expect(store.get(currentTrackAtom)?.provider).toBe("deezer");
  });

  it("plays a TIDAL (untagged) track with play_tidal_track", async () => {
    const { result } = setup();
    await act(async () => {
      await result.current.playTrack(track());
    });
    expect(calls("play_tidal_track")).toHaveLength(1);
    expect(calls("play_deezer_track")).toHaveLength(0);
  });

  it("replays a finished Deezer track through the Deezer command on resume", async () => {
    const { store, result } = setup();
    store.set(currentTrackAtom, track({ provider: "deezer" }));
    await act(async () => {
      await result.current.resumeTrack();
    });
    expect(calls("play_deezer_track")).toHaveLength(1);
    expect(calls("play_tidal_track")).toHaveLength(0);
  });

  it("does not report a Deezer play to TIDAL under its Deezer id", async () => {
    const { result } = setup();
    await act(async () => {
      await result.current.playTrack(track({ provider: "deezer" }));
    });
    const [[, arg]] = calls("notify_track_started");
    const payload = (arg as { payload: Record<string, unknown> }).payload;
    expect(payload.trackId).toBeNull();
    expect(payload.sourceType).toBeNull();
    expect(payload.sourceId).toBeNull();
    // Scrobblers still get the metadata.
    expect(payload.title).toBe("T");
  });
});

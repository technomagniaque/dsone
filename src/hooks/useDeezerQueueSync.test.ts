import { describe, it, expect } from "vitest";
import { createStore } from "jotai";
import {
  applyDeezerCloudQueue,
  buildQueueSignature,
  collectDeezerQueue,
} from "./useDeezerQueueSync";
import {
  currentTrackAtom,
  historyAtom,
  queueAtom,
  repeatAtom,
  shuffleAtom,
} from "../atoms/playback";
import { deezerQueueSyncStatusAtom } from "../atoms/deezer";
import type { Track } from "../types";

const dzTrack = (id: number): Track => ({
  id,
  title: `Track ${id}`,
  duration: 200,
  provider: "deezer",
});

describe("useDeezerQueueSync helpers", () => {
  it("applies a cloud queue at the exact synced position", () => {
    const store = createStore();
    const active = applyDeezerCloudQueue(store, {
      tracks: [dzTrack(1), dzTrack(2), dzTrack(3), dzTrack(4)],
      position: 2,
      repeatMode: 1,
      shuffled: true,
      version: "ver-9",
      localQueueId: "android-q",
      sourceUserAgent: "Deezer Android",
    });

    expect(active?.id).toBe(3);
    expect(store.get(currentTrackAtom)?.id).toBe(3);
    expect(store.get(historyAtom).map((t) => t.id)).toEqual([1, 2]);
    expect(store.get(queueAtom).map((t) => t.id)).toEqual([4]);
    expect(store.get(repeatAtom)).toBe(1);
    expect(store.get(shuffleAtom)).toBe(true);
    expect(store.get(deezerQueueSyncStatusAtom).version).toBe("ver-9");
  });

  it("collects history + current + queue back into a single Deezer track list", () => {
    const collected = collectDeezerQueue(
      [dzTrack(10), dzTrack(11)],
      dzTrack(12),
      [dzTrack(13)],
    );
    expect(collected).toEqual({
      trackIds: [10, 11, 12, 13],
      position: 2,
    });
    expect(
      collectDeezerQueue([], { id: 99, title: "TIDAL", duration: 100 }, []),
    ).toBeNull();
  });

  it("produces deterministic queue signatures for Deezer tracks only", () => {
    const sig1 = buildQueueSignature([dzTrack(1)], dzTrack(2), [dzTrack(3)], 0, false);
    const sig2 = buildQueueSignature([dzTrack(1)], dzTrack(2), [dzTrack(3)], 1, false);
    expect(sig1).not.toBeNull();
    expect(sig1).not.toBe(sig2);
    expect(
      buildQueueSignature([], { id: 99, title: "TIDAL", duration: 100 }, [], 0, false),
    ).toBeNull();
  });
});

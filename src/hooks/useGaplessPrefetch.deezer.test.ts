import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { cleanup, renderHook, waitFor } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import React from "react";
import { invoke } from "@tauri-apps/api/core";
import { useGaplessPrefetch } from "./useGaplessPrefetch";
import { currentTrackAtom, queueAtom, gaplessAtom } from "../atoms/playback";
import type { Track } from "../types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

afterEach(cleanup);

function setup(store: ReturnType<typeof createStore>, predict: () => Track) {
  const pendingNextRef = { current: null };
  const wrapper = ({ children }: { children: React.ReactNode }) =>
    React.createElement(Provider, { store }, children);
  return renderHook(
    () => useGaplessPrefetch(predict, pendingNextRef as never),
    { wrapper },
  );
}

const calls = (cmd: string) =>
  vi.mocked(invoke).mock.calls.filter((c) => c[0] === cmd);

describe("useGaplessPrefetch provider dispatch", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation(async (cmd: string) =>
      cmd === "get_gapless_supported" ? true : {},
    );
  });

  it("arms a Deezer next track with set_next_deezer_track", async () => {
    const store = createStore();
    store.set(gaplessAtom, true);
    store.set(currentTrackAtom, { id: 1 } as unknown as Track);
    const next = {
      id: 3135556,
      title: "dz",
      _qid: "qdz",
      provider: "deezer",
    } as unknown as Track;

    setup(store, () => next);
    await waitFor(() => expect(calls("set_next_deezer_track")).toHaveLength(1));
    expect(calls("set_next_deezer_track")[0][1]).toMatchObject({
      trackId: 3135556,
      qid: "qdz",
    });
    expect(calls("set_next_track")).toHaveLength(0);
  });

  it("re-arms when the next track keeps its id and qid but changes service", async () => {
    const store = createStore();
    store.set(gaplessAtom, true);
    store.set(currentTrackAtom, { id: 1 } as unknown as Track);
    const tidal = { id: 9, title: "t", _qid: "same" } as unknown as Track;
    const deezer = { ...tidal, provider: "deezer" } as Track;
    let next = tidal;

    setup(store, () => next);
    await waitFor(() => expect(calls("set_next_track")).toHaveLength(1));

    next = deezer;
    store.set(queueAtom, [deezer]);
    await waitFor(() => expect(calls("set_next_deezer_track")).toHaveLength(1));
  });
});

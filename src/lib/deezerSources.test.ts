import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  deezerArtistRadioPlayable,
  deezerTrackMixPlayable,
  deezerViewForSource,
} from "./deezerSources";
import type { Track } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const SEED: Track = {
  id: 3135556,
  title: "Harder, Better, Faster, Stronger",
  duration: 224,
  artist: { id: 27, name: "Daft Punk" },
  album: { id: 302127, title: "Discovery", cover: "https://x/c.jpg" },
  provider: "deezer",
};

describe("deezerSources radio & mix playables", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("places the seed track first and deduplicates it in deezerTrackMixPlayable", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([
      { id: 3135556, title: "Harder, Better, Faster, Stronger", duration: 224 },
      { id: 3135553, title: "One More Time", duration: 320 },
    ]);

    const src = await deezerTrackMixPlayable(SEED);
    expect(invoke).toHaveBeenCalledWith("deezer_get_track_mix", {
      trackId: 3135556,
    });
    expect(src.meta).toEqual({
      type: "deezer-artist",
      id: 27,
      name: "Harder, Better, Faster, Stronger Radio",
      image: "https://x/c.jpg",
    });
    expect(src.loaded.map((t) => t.id)).toEqual([3135556, 3135553]);
    expect(src.loaded.every((t) => t.provider === "deezer")).toBe(true);
  });

  it("builds an artist radio playable from deezer_get_artist_radio", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([
      { id: 3135553, title: "One More Time", duration: 320 },
    ]);

    const src = await deezerArtistRadioPlayable(
      27,
      "Daft Punk",
      "https://x/a.jpg",
    );
    expect(invoke).toHaveBeenCalledWith("deezer_get_artist_radio", {
      artistId: 27,
    });
    expect(src.meta).toEqual({
      type: "deezer-artist",
      id: 27,
      name: "Daft Punk Radio",
      image: "https://x/a.jpg",
    });
    expect(src.loaded).toHaveLength(1);
    expect(src.loaded[0].provider).toBe("deezer");
    expect(deezerViewForSource(src.meta)).toEqual({
      type: "artist",
      artistId: 27,
      artistInfo: { name: "Daft Punk Radio", picture: "https://x/a.jpg" },
      provider: "deezer",
    });
  });
});

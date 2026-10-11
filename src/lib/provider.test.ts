import { describe, it, expect } from "vitest";
import {
  isDeezer,
  isSameTrack,
  nextTrackCommandFor,
  playCommandFor,
  resolveSearchProvider,
} from "./provider";

describe("provider dispatch", () => {
  it("plays Deezer tracks through the Deezer commands", () => {
    const t = { id: 3135556, provider: "deezer" as const };
    expect(isDeezer(t)).toBe(true);
    expect(playCommandFor(t)).toBe("play_deezer_track");
    expect(nextTrackCommandFor(t)).toBe("set_next_deezer_track");
  });

  it("keeps TIDAL (and untagged legacy) tracks on the TIDAL commands", () => {
    for (const t of [{ id: 1 }, { id: 1, provider: "tidal" as const }]) {
      expect(isDeezer(t)).toBe(false);
      expect(playCommandFor(t)).toBe("play_tidal_track");
      expect(nextTrackCommandFor(t)).toBe("set_next_track");
    }
    expect(isDeezer(null)).toBe(false);
  });

  it("never treats a TIDAL and a Deezer track with the same id as one", () => {
    expect(isSameTrack({ id: 7 }, { id: 7, provider: "deezer" })).toBe(false);
    expect(isSameTrack({ id: 7 }, { id: 7, provider: "tidal" })).toBe(true);
    expect(
      isSameTrack({ id: 7, provider: "deezer" }, { id: 7, provider: "deezer" }),
    ).toBe(true);
    expect(isSameTrack(null, { id: 7 })).toBe(false);
  });
});

describe("resolveSearchProvider", () => {
  it("searches Deezer by default when only Deezer is signed in", () => {
    expect(resolveSearchProvider(null, false, true)).toBe("deezer");
    expect(resolveSearchProvider("tidal", false, true)).toBe("deezer");
  });

  it("searches TIDAL when Deezer is not signed in", () => {
    expect(resolveSearchProvider("deezer", true, false)).toBe("tidal");
  });

  it("honours the user's pick when both are signed in, TIDAL otherwise", () => {
    expect(resolveSearchProvider(null, true, true)).toBe("tidal");
    expect(resolveSearchProvider("deezer", true, true)).toBe("deezer");
  });
});

import { describe, expect, it } from "vitest";
import { parseDeezerAutologUrl } from "./deezerLogin";

const ARL = "a".repeat(64) + "B9";

describe("parseDeezerAutologUrl", () => {
  it("extracts the ARL from Deezer's autolog link", () => {
    expect(parseDeezerAutologUrl(`deezer://autolog/${ARL}`)).toBe(ARL);
  });

  it("ignores a trailing slash, query or fragment", () => {
    expect(parseDeezerAutologUrl(`deezer://autolog/${ARL}/`)).toBe(ARL);
    expect(parseDeezerAutologUrl(`deezer://autolog/${ARL}?x=1`)).toBe(ARL);
    expect(parseDeezerAutologUrl(`deezer://autolog/${ARL}#top`)).toBe(ARL);
  });

  it("rejects other links and malformed ARLs", () => {
    expect(parseDeezerAutologUrl("tidal://track/1")).toBeNull();
    expect(parseDeezerAutologUrl("deezer://track/1")).toBeNull();
    expect(parseDeezerAutologUrl("deezer://autolog/short")).toBeNull();
    expect(
      parseDeezerAutologUrl(`deezer://autolog/${"x".repeat(40)} ;rm`),
    ).toBeNull();
  });
});

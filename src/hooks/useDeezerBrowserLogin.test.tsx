import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, renderHook, waitFor } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import type { PropsWithChildren } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrent, onOpenUrl } from "@tauri-apps/plugin-deep-link";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-deep-link", () => ({
  getCurrent: vi.fn(),
  onOpenUrl: vi.fn(),
}));

import { useDeezerAutologListener } from "./useDeezerBrowserLogin";
import { deezerBrowserLoginAtom, deezerUserAtom } from "../atoms/deezer";

const ARL = "f".repeat(150);
const USER = { id: 7, name: "E", canStreamLossless: true, canStreamHq: true };

function mount() {
  const store = createStore();
  const wrapper = ({ children }: PropsWithChildren) => (
    <Provider store={store}>{children}</Provider>
  );
  renderHook(() => useDeezerAutologListener(), { wrapper });
  return store;
}

describe("useDeezerAutologListener", () => {
  let deliver: (urls: string[]) => void = () => {};

  afterEach(cleanup);
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(getCurrent).mockResolvedValue(null);
    vi.mocked(onOpenUrl).mockImplementation(async (cb) => {
      deliver = cb;
      return () => {};
    });
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "deezer_login") return USER;
      if (cmd === "deezer_get_favorite_tracks") return [];
      return null;
    });
  });

  it("signs in with the ARL from a deezer://autolog link", async () => {
    const store = mount();
    await waitFor(() => expect(onOpenUrl).toHaveBeenCalled());
    deliver([`deezer://autolog/${ARL}`]);
    await waitFor(() => expect(store.get(deezerUserAtom)).toEqual(USER));
    expect(invoke).toHaveBeenCalledWith("deezer_login", { arl: ARL });
    expect(store.get(deezerBrowserLoginAtom)).toEqual({ state: "idle" });
  });

  it("handles a cold start launched by the link", async () => {
    vi.mocked(getCurrent).mockResolvedValue([`deezer://autolog/${ARL}`]);
    const store = mount();
    await waitFor(() => expect(store.get(deezerUserAtom)).toEqual(USER));
  });

  it("ignores TIDAL links", async () => {
    mount();
    await waitFor(() => expect(onOpenUrl).toHaveBeenCalled());
    deliver(["tidal://track/1"]);
    expect(invoke).not.toHaveBeenCalledWith("deezer_login", expect.anything());
  });

  it("reports a rejected ARL", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "deezer_login")
        throw JSON.stringify({ kind: "Api", message: "Invalid ARL" });
      return null;
    });
    const store = mount();
    await waitFor(() => expect(onOpenUrl).toHaveBeenCalled());
    deliver([`deezer://autolog/${ARL}`]);
    await waitFor(() =>
      expect(store.get(deezerBrowserLoginAtom)).toEqual({
        state: "error",
        message: "Invalid ARL",
      }),
    );
  });
});

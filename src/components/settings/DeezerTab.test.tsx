import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import type { PropsWithChildren } from "react";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));
vi.mock("../../contexts/ToastContext", () => ({
  useToast: () => ({ showToast: vi.fn() }),
}));

import DeezerTab from "./DeezerTab";
import { deezerQualityAtom, deezerUserAtom } from "../../atoms/deezer";

describe("DeezerTab Hi-Fi 24-bit / 192 kHz quality picker", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      if (cmd === "get_deezer_quality") return "FLAC";
      return null;
    });
  });

  afterEach(() => {
    cleanup();
    vi.mocked(invoke).mockReset();
  });

  it("displays Hi-Fi / Hi-Res Lossless up to 24-bit / 192 kHz for lossless accounts", () => {
    const store = createStore();
    store.set(deezerUserAtom, {
      id: 42,
      name: "Etienne",
      canStreamLossless: true,
      canStreamHq: true,
      country: "CA",
    });
    store.set(deezerQualityAtom, "FLAC");

    const wrapper = ({ children }: PropsWithChildren) => (
      <Provider store={store}>{children}</Provider>
    );
    render(<DeezerTab />, { wrapper });

    expect(
      screen.getByText(/Hi-Fi Lossless \(up to 24-bit \/ 192 kHz\)/),
    ).toBeTruthy();
    expect(screen.getByText("Hi-Fi / Hi-Res Lossless")).toBeTruthy();
    expect(
      screen.getByText("FLAC · 16-bit / 44.1 kHz up to 24-bit / 192 kHz"),
    ).toBeTruthy();

    fireEvent.click(screen.getByText("High"));
    expect(invoke).toHaveBeenCalledWith("set_deezer_quality", {
      quality: "MP3_320",
    });

    expect(screen.getByText(/Playback sync/i)).toBeTruthy();
    expect(screen.getByText("Sync from Deezer")).toBeTruthy();
    expect(screen.getByText("Push queue")).toBeTruthy();
  });
});

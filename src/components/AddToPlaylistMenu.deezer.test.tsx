import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { createRef, type PropsWithChildren } from "react";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const showToast = vi.fn();
vi.mock("../contexts/ToastContext", () => ({
  useToast: () => ({ showToast }),
}));

import AddToPlaylistMenu from "./AddToPlaylistMenu";
import { deezerPlaylistsVersionAtom } from "../atoms/deezer";

const PLAYLISTS = [
  { uuid: "11", title: "Road trip", numberOfTracks: 4, provider: "deezer" },
  { uuid: "12", title: "Late night", numberOfTracks: 9, provider: "deezer" },
];

function renderMenu() {
  const store = createStore();
  const wrapper = ({ children }: PropsWithChildren) => (
    <Provider store={store}>{children}</Provider>
  );
  const anchor = createRef<HTMLButtonElement>();
  const onClose = vi.fn();
  render(
    <AddToPlaylistMenu
      trackIds={[3135556]}
      provider="deezer"
      anchorRef={anchor}
      onClose={onClose}
    />,
    { wrapper },
  );
  return { store, onClose };
}

describe("AddToPlaylistMenu (Deezer)", () => {
  afterEach(cleanup);
  beforeEach(() => {
    localStorage.clear();
    showToast.mockReset();
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "deezer_get_editable_playlists") return PLAYLISTS;
      return null;
    });
  });

  it("lists the user's Deezer playlists, not TIDAL's", async () => {
    renderMenu();
    expect(await screen.findByText("Road trip")).toBeTruthy();
    expect(screen.getByText("Late night")).toBeTruthy();
    expect(invoke).not.toHaveBeenCalledWith(
      "get_all_playlists",
      expect.anything(),
    );
  });

  it("adds the track to the picked Deezer playlist", async () => {
    const { store } = renderMenu();
    fireEvent.click(await screen.findByText("Late night"));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("deezer_add_tracks_to_playlist", {
        playlistId: 12,
        trackIds: [3135556],
      }),
    );
    await waitFor(() =>
      expect(showToast).toHaveBeenCalledWith('Added to "Late night"'),
    );
    expect(store.get(deezerPlaylistsVersionAtom)).toBe(1);
  });

  it("says when the track is already in the playlist", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "deezer_get_editable_playlists") return PLAYLISTS;
      if (cmd === "deezer_add_tracks_to_playlist")
        throw JSON.stringify({
          kind: "Api",
          message: {
            status: 400,
            body: "deezer playlist.addSongs: ERROR_DATA_EXISTS",
          },
        });
      return null;
    });
    renderMenu();
    fireEvent.click(await screen.findByText("Road trip"));
    expect(
      await screen.findByText("Track already in this playlist"),
    ).toBeTruthy();
  });
});

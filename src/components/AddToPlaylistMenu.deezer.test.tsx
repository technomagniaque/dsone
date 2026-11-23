import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import {
  render,
  screen,
  fireEvent,
  waitFor,
  cleanup,
} from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { invoke } from "@tauri-apps/api/core";
import AddToPlaylistMenu from "./AddToPlaylistMenu";
import { ToastProvider } from "../contexts/ToastContext";
import { deezerUserAtom } from "../atoms/deezer";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

afterEach(cleanup);

function renderMenu(anchorEl: HTMLButtonElement) {
  const store = createStore();
  store.set(deezerUserAtom, {
    id: 1,
    name: "Tester",
    canStreamLossless: true,
    canStreamHq: true,
  });
  return render(
    <Provider store={store}>
      <ToastProvider>
        <AddToPlaylistMenu
          trackIds={[999]}
          provider="deezer"
          anchorRef={{ current: anchorEl }}
          onClose={() => {}}
        />
      </ToastProvider>
    </Provider>,
  );
}

describe("AddToPlaylistMenu (Deezer errors)", () => {
  let anchor: HTMLButtonElement;

  beforeEach(() => {
    localStorage.clear();
    vi.mocked(invoke).mockReset();
    anchor = document.createElement("button");
    document.body.appendChild(anchor);
  });

  afterEach(() => {
    anchor.remove();
  });

  it("reports duplicate Deezer track when SoneError::Api arrives as an object", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "deezer_get_editable_playlists") {
        return [
          {
            uuid: "12345",
            title: "df1",
            numberOfTracks: 84,
            provider: "deezer",
          },
        ];
      }
      if (cmd === "deezer_add_tracks_to_playlist") {
        throw {
          kind: "Api",
          message: {
            status: 400,
            body: 'deezer playlist.addSongs: {"ERROR_DATA_EXISTS":"This data already exists"}',
          },
        };
      }
      return null;
    });

    renderMenu(anchor);
    await screen.findByText("df1");
    await waitFor(() => {
      const btn = screen.getByText("df1").closest("button")!;
      expect(btn.isConnected).toBe(true);
    });
    // Wait a tick for useContextMenu positioning re-render before clicking
    await new Promise((r) => setTimeout(r, 20));
    fireEvent.click(screen.getByText("df1").closest("button")!);

    await waitFor(() => {
      expect(
        screen.getByText("Track already in this playlist"),
      ).toBeTruthy();
    });
  });

  it("surfaces real API error messages instead of a generic failure", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "deezer_get_editable_playlists") {
        return [
          {
            uuid: "12345",
            title: "df1",
            numberOfTracks: 84,
            provider: "deezer",
          },
        ];
      }
      if (cmd === "deezer_add_tracks_to_playlist") {
        throw {
          kind: "Api",
          message: {
            status: 400,
            body: "deezer playlist.addSongs: QUOTA_ERROR",
          },
        };
      }
      return null;
    });

    renderMenu(anchor);
    await screen.findByText("df1");
    await new Promise((r) => setTimeout(r, 20));
    fireEvent.click(screen.getByText("df1").closest("button")!);

    await waitFor(() => {
      expect(
        screen.getByText("Playlist track limit reached"),
      ).toBeTruthy();
    });
  });
});

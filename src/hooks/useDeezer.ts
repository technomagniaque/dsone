import { useCallback } from "react";
import { useAtomValue, useStore } from "jotai";
import { invoke } from "@tauri-apps/api/core";
import {
  deezerFavoriteAlbumIdsAtom,
  deezerFavoriteArtistIdsAtom,
  deezerFavoritePlaylistIdsAtom,
  deezerFavoriteTrackIdsAtom,
  deezerUserAtom,
  type DeezerUser,
} from "../atoms/deezer";
import {
  currentTrackAtom,
  isPlayingAtom,
  manualQueueAtom,
  queueAtom,
  streamInfoAtom,
} from "../atoms/playback";
import { currentViewAtom } from "../atoms/navigation";
import {
  deezerGetFavoriteTracks,
  deezerGetUserAlbums,
  deezerGetUserArtists,
  deezerGetUserPlaylists,
  deezerLogin,
  deezerLogout,
} from "../api/deezer";
import { isDeezer } from "../lib/provider";

/** Fill the Deezer favourite-id sets (non-blocking, errors only logged). */
export function loadDeezerFavoriteIds(store: ReturnType<typeof useStore>) {
  deezerGetFavoriteTracks()
    .then((tracks) =>
      store.set(deezerFavoriteTrackIdsAtom, new Set(tracks.map((t) => t.id))),
    )
    .catch((err) => console.error("Failed to load Deezer favorites:", err));
  deezerGetUserAlbums()
    .then((albums) =>
      store.set(deezerFavoriteAlbumIdsAtom, new Set(albums.map((a) => a.id))),
    )
    .catch((err) =>
      console.error("Failed to load Deezer favorite albums:", err),
    );
  deezerGetUserArtists()
    .then((artists) =>
      store.set(deezerFavoriteArtistIdsAtom, new Set(artists.map((a) => a.id))),
    )
    .catch((err) =>
      console.error("Failed to load Deezer favorite artists:", err),
    );
  deezerGetUserPlaylists()
    .then((playlists) =>
      store.set(
        deezerFavoritePlaylistIdsAtom,
        new Set(playlists.map((p) => p.uuid)),
      ),
    )
    .catch((err) =>
      console.error("Failed to load Deezer favorite playlists:", err),
    );
}

export function useDeezer() {
  const store = useStore();
  const user = useAtomValue(deezerUserAtom);

  const login = useCallback(
    async (arl: string): Promise<DeezerUser> => {
      const u = await deezerLogin(arl.trim());
      store.set(deezerUserAtom, u);
      loadDeezerFavoriteIds(store);
      return u;
    },
    [store],
  );

  const logout = useCallback(async () => {
    // Drop Deezer audio first so nothing keeps streaming through a session
    // the backend is about to forget.
    if (isDeezer(store.get(currentTrackAtom))) {
      await invoke("stop_track").catch(() => {});
      store.set(isPlayingAtom, false);
      store.set(currentTrackAtom, null);
      store.set(streamInfoAtom, null);
    }
    store.set(
      queueAtom,
      store.get(queueAtom).filter((t) => !isDeezer(t)),
    );
    store.set(
      manualQueueAtom,
      store.get(manualQueueAtom).filter((t) => !isDeezer(t)),
    );
    try {
      await deezerLogout();
    } catch (err) {
      console.error("Failed to log out of Deezer:", err);
    } finally {
      store.set(deezerUserAtom, null);
      store.set(deezerFavoriteTrackIdsAtom, new Set<number>());
      store.set(deezerFavoriteAlbumIdsAtom, new Set<number>());
      store.set(deezerFavoriteArtistIdsAtom, new Set<number>());
      store.set(deezerFavoritePlaylistIdsAtom, new Set<string>());
      store.set(currentViewAtom, { type: "home" });
    }
  }, [store]);

  return { user, login, logout };
}

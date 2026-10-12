import { invoke } from "@tauri-apps/api/core";
import type { DeezerQuality, DeezerUser } from "../atoms/deezer";
import type { AlbumDetail, Playlist, SearchResults, Track } from "../types";

/** Deezer artist header, as returned by `deezer_get_artist`. */
export interface DeezerArtist {
  id: number;
  name: string;
  picture?: string;
  provider?: "deezer";
}

export interface DeezerArtistPage {
  artist: DeezerArtist;
  topTracks: Track[];
  albums: AlbumDetail[];
}

// The backend tags every item with `provider: "deezer"`. Re-tagging here costs
// nothing and guarantees playback never sends a Deezer id to a TIDAL command,
// even if one nested item slips through untagged.
const tag = <T extends object>(item: T): T & { provider: "deezer" } => ({
  ...item,
  provider: "deezer",
});
const tagAll = <T extends object>(items: T[] | null | undefined) =>
  (items ?? []).map(tag);

export const deezerLogin = (arl: string) =>
  invoke<DeezerUser>("deezer_login", { arl });

export const deezerRestoreSession = () =>
  invoke<DeezerUser | null>("deezer_restore_session");

export const deezerLogout = () => invoke<null>("deezer_logout");

export async function deezerSearch(
  query: string,
  limit?: number,
): Promise<SearchResults> {
  const r = await invoke<SearchResults>("deezer_search", { query, limit });
  return {
    artists: tagAll(r.artists),
    albums: tagAll(r.albums),
    tracks: tagAll(r.tracks),
    playlists: tagAll(r.playlists),
    videos: [],
  };
}

export async function deezerGetAlbum(
  albumId: number,
): Promise<{ album: AlbumDetail; tracks: Track[] }> {
  const r = await invoke<{ album: AlbumDetail; tracks: Track[] }>(
    "deezer_get_album",
    { albumId },
  );
  return { album: tag(r.album), tracks: tagAll(r.tracks) };
}

export async function deezerGetPlaylist(
  playlistId: number,
): Promise<{ playlist: Playlist; tracks: Track[] }> {
  const r = await invoke<{ playlist: Playlist; tracks: Track[] }>(
    "deezer_get_playlist",
    { playlistId },
  );
  return { playlist: tag(r.playlist), tracks: tagAll(r.tracks) };
}

export async function deezerGetArtist(
  artistId: number,
): Promise<DeezerArtistPage> {
  const r = await invoke<DeezerArtistPage>("deezer_get_artist", { artistId });
  return {
    artist: tag(r.artist),
    topTracks: tagAll(r.topTracks),
    albums: tagAll(r.albums),
  };
}

export const deezerGetFavoriteTracks = async () =>
  tagAll(await invoke<Track[]>("deezer_get_favorite_tracks"));

export const deezerGetUserPlaylists = async () =>
  tagAll(await invoke<Playlist[]>("deezer_get_user_playlists"));

/** The user's own playlists, minus Loved Tracks: the ones tracks can be
 *  added to. */
export const deezerGetEditablePlaylists = async () =>
  tagAll(await invoke<Playlist[]>("deezer_get_editable_playlists"));

export const deezerAddTracksToPlaylist = (
  playlistId: number,
  trackIds: number[],
) => invoke<null>("deezer_add_tracks_to_playlist", { playlistId, trackIds });

/** Returns the new playlist's id. */
export const deezerCreatePlaylist = (
  title: string,
  description: string,
  isPublic: boolean,
  trackIds: number[],
) =>
  invoke<number>("deezer_create_playlist", {
    title,
    description,
    isPublic,
    trackIds,
  });

export const deezerGetUserAlbums = async () =>
  tagAll(await invoke<AlbumDetail[]>("deezer_get_user_albums"));

export const deezerGetFlow = async () =>
  tagAll(await invoke<Track[]>("deezer_get_flow"));

export const deezerSetFavoriteTrack = (trackId: number, favorite: boolean) =>
  invoke<null>("deezer_set_favorite_track", { trackId, favorite });

export const getDeezerQuality = () =>
  invoke<DeezerQuality>("get_deezer_quality");

export const setDeezerQuality = (quality: DeezerQuality) =>
  invoke<null>("set_deezer_quality", { quality });

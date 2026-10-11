import {
  deezerGetAlbum,
  deezerGetArtist,
  deezerGetFavoriteTracks,
  deezerGetFlow,
  deezerGetPlaylist,
} from "../api/deezer";
import type { AppView } from "../types";
import type { PlayableSource } from "./trackSources";

/** Playback-source `type`s for Deezer containers. Distinct from the TIDAL
 *  ones so "Playing from" never routes a Deezer id to a TIDAL page and the
 *  play buttons never confuse a TIDAL album with a Deezer one of the same id. */
export const DEEZER_SOURCE_TYPES = [
  "deezer-album",
  "deezer-playlist",
  "deezer-artist",
  "deezer-favorites",
  "deezer-flow",
] as const;

type DeezerSourceType = (typeof DEEZER_SOURCE_TYPES)[number];

interface SourceRef {
  type: string;
  id: string | number;
  name: string;
  image?: string;
}

/** The page a Deezer playback source navigates back to, or null when the
 *  source is not a Deezer one. */
export function deezerViewForSource(source: SourceRef): AppView | null {
  switch (source.type as DeezerSourceType) {
    case "deezer-album":
      return {
        type: "album",
        albumId: Number(source.id),
        albumInfo: { title: source.name, cover: source.image },
        provider: "deezer",
      };
    case "deezer-playlist":
      return {
        type: "playlist",
        playlistId: String(source.id),
        playlistInfo: { title: source.name, image: source.image },
        provider: "deezer",
      };
    case "deezer-artist":
      return {
        type: "artist",
        artistId: Number(source.id),
        artistInfo: { name: source.name, picture: source.image },
        provider: "deezer",
      };
    case "deezer-favorites":
      return { type: "deezerFavorites" };
    case "deezer-flow":
      return { type: "deezerFlow" };
    default:
      return null;
  }
}

export type DeezerMediaKind = "album" | "playlist" | "artist";

/** Load a Deezer album / playlist / artist as a complete playable source. */
export async function deezerPlayable(
  kind: DeezerMediaKind,
  id: number | string,
  name: string,
  image?: string,
): Promise<PlayableSource> {
  switch (kind) {
    case "album": {
      const { album, tracks } = await deezerGetAlbum(Number(id));
      return {
        meta: {
          type: "deezer-album",
          id: Number(id),
          name: album.title || name,
          image: album.cover ?? image,
        },
        loaded: tracks,
        albumMode: true,
      };
    }
    case "playlist": {
      const { playlist, tracks } = await deezerGetPlaylist(Number(id));
      return {
        meta: {
          type: "deezer-playlist",
          id: String(id),
          name: playlist.title || name,
          image: playlist.squareImage || playlist.image || image,
        },
        loaded: tracks,
      };
    }
    case "artist": {
      const { artist, topTracks } = await deezerGetArtist(Number(id));
      return {
        meta: {
          type: "deezer-artist",
          id: Number(id),
          name: artist.name || name,
          image: artist.picture ?? image,
        },
        loaded: topTracks,
      };
    }
  }
}

export async function deezerFavoritesPlayable(): Promise<PlayableSource> {
  return {
    meta: {
      type: "deezer-favorites",
      id: "deezer-favorites",
      name: "Loved Tracks",
    },
    loaded: await deezerGetFavoriteTracks(),
  };
}

export async function deezerFlowPlayable(): Promise<PlayableSource> {
  return {
    meta: { type: "deezer-flow", id: "deezer-flow", name: "Flow" },
    loaded: await deezerGetFlow(),
  };
}

const DEEZER_SHARE_BASE = "https://www.deezer.com";

export function getDeezerShareUrl(
  kind: "track" | DeezerMediaKind,
  id: number | string,
): string {
  return `${DEEZER_SHARE_BASE}/${kind}/${id}`;
}

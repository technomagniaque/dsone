import { atom } from "jotai";
import { atomWithStorage } from "jotai/utils";
import type { Provider } from "../types";

export interface DeezerUser {
  id: number;
  name: string;
  picture?: string;
  canStreamLossless: boolean;
  canStreamHq: boolean;
  country?: string;
}

export type DeezerQuality = "FLAC" | "MP3_320" | "MP3_128";

/** Logged-in Deezer account, or null. Independent of the TIDAL session. */
export const deezerUserAtom = atom<DeezerUser | null>(null);
/** True until `deezer_restore_session` resolves at startup. */
export const deezerAuthCheckingAtom = atom(true);
/** Deezer favourite ("Coups de cœur") track ids — separate from TIDAL ids,
 *  since the two id spaces overlap. */
export const deezerFavoriteTrackIdsAtom = atom<Set<number>>(new Set<number>());
export const deezerFavoriteAlbumIdsAtom = atom<Set<number>>(new Set<number>());
export const deezerFavoriteArtistIdsAtom = atom<Set<number>>(new Set<number>());
export const deezerFavoritePlaylistIdsAtom = atom<Set<string>>(
  new Set<string>(),
);
/** Sync active Deezer play queue & current track with other Deezer clients
 *  (Android / iOS / Web / Desktop) via Deezer Cloud Queue. Enabled by default. */
export const deezerCloudQueueSyncAtom = atomWithStorage<boolean>(
  "sone.deezer-cloud-queue-sync.v1",
  true,
);
export interface DeezerQueueSyncStatus {
  version: string | null;
  localQueueId: string | null;
  lastSyncedAt: number | null;
  syncing: boolean;
  lastSource: string | null;
  lastError?: string | null;
}
export const deezerQueueSyncStatusAtom = atom<DeezerQueueSyncStatus>({
  version: null,
  localQueueId: null,
  lastSyncedAt: null,
  syncing: false,
  lastSource: null,
  lastError: null,
});
/** Currently selected Deezer Flow mood/genre config_id ("default" = standard Flow). */
export const deezerFlowMoodAtom = atom<string>("default");
/** Progress of the browser sign-in (Deezer's page → deezer://autolog link). */
export type DeezerBrowserLogin =
  | { state: "idle" }
  | { state: "waiting" }
  | { state: "signing-in" }
  | { state: "error"; message: string };
export const deezerBrowserLoginAtom = atom<DeezerBrowserLogin>({
  state: "idle",
});
/** Bumped after a Deezer playlist is created or changed, so lists refetch. */
export const deezerPlaylistsVersionAtom = atom(0);
export const deezerQualityAtom = atom<DeezerQuality>("FLAC");
/** Search source picked by the user; null = automatic (TIDAL when signed in). */
export const searchProviderAtom = atomWithStorage<Provider | null>(
  "sone.search-provider.v1",
  null,
);

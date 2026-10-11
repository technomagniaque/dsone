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
export const deezerQualityAtom = atom<DeezerQuality>("FLAC");
/** Search source picked by the user; null = automatic (TIDAL when signed in). */
export const searchProviderAtom = atomWithStorage<Provider | null>(
  "sone.search-provider.v1",
  null,
);

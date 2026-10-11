import { DEEZER_SOURCE_TYPES } from "./deezerSources";

export const NAVIGABLE_SOURCE_TYPES = new Set<string>([
  "album",
  "playlist",
  "playlist-recs",
  "mix",
  "artist",
  "artist-tracks",
  "favorites",
  "radio",
  ...DEEZER_SOURCE_TYPES,
]);

export const isNavigableSource = (type?: string): boolean =>
  !!type && NAVIGABLE_SOURCE_TYPES.has(type);

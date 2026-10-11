import type { Provider } from "../types";

type WithProvider = { provider?: Provider } | null | undefined;

/** True when an item (track, album, playlist, artist) comes from Deezer. */
export const isDeezer = (item: WithProvider): boolean =>
  item?.provider === "deezer";

/** Backend command that starts playback of `track` right now. */
export const playCommandFor = (
  track: WithProvider,
): "play_deezer_track" | "play_tidal_track" =>
  isDeezer(track) ? "play_deezer_track" : "play_tidal_track";

/** Backend command that arms `track` as the gapless next track. */
export const nextTrackCommandFor = (
  track: WithProvider,
): "set_next_deezer_track" | "set_next_track" =>
  isDeezer(track) ? "set_next_deezer_track" : "set_next_track";

/** Same catalogue entry: TIDAL and Deezer ids live in overlapping spaces, so an
 *  id match alone is not enough. */
export const isSameTrack = (
  a: ({ id: number } & { provider?: Provider }) | null | undefined,
  b: ({ id: number } & { provider?: Provider }) | null | undefined,
): boolean => !!a && !!b && a.id === b.id && isDeezer(a) === isDeezer(b);

/** Which service a search runs against, given who is signed in. */
export function resolveSearchProvider(
  requested: Provider | null | undefined,
  tidalSignedIn: boolean,
  deezerSignedIn: boolean,
): Provider {
  if (!deezerSignedIn) return "tidal";
  if (!tidalSignedIn) return "deezer";
  return requested ?? "tidal";
}

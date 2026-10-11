import { useCallback } from "react";
import { useToast } from "../contexts/ToastContext";
import { deezerPlayable, type DeezerMediaKind } from "../lib/deezerSources";
import { usePlaySource } from "./usePlaySource";

/** Play a Deezer album / playlist / artist straight from a card. */
export function useDeezerMediaPlay() {
  const playSource = usePlaySource();
  const { showToast } = useToast();
  return useCallback(
    async (
      kind: DeezerMediaKind,
      id: number | string,
      name: string,
      image?: string,
    ) => {
      try {
        await playSource(await deezerPlayable(kind, id, name, image));
      } catch (err) {
        console.error(`Failed to play Deezer ${kind}:`, err);
        showToast("Couldn't load tracks", "error");
      }
    },
    [playSource, showToast],
  );
}

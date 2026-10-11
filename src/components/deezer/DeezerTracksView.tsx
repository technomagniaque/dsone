import { useEffect, useState } from "react";
import { Heart, Waves } from "lucide-react";
import { useAtom } from "jotai";
import { useNavigation } from "../../hooks/useNavigation";
import { deezerGetFavoriteTracks, deezerGetFlow } from "../../api/deezer";
import { deezerFavoriteTrackIdsAtom } from "../../atoms/deezer";
import { safeErrorMessage } from "../../lib/errorUtils";
import type { Track } from "../../types";
import DeezerCollectionPage from "./DeezerCollectionPage";

/** Gradient tile used for Deezer's personal collections (no cover art). */
export function DeezerCollectionTile({
  kind,
  size = 56,
}: {
  kind: "favorites" | "flow";
  size?: number;
}) {
  return (
    <div
      className={`w-full h-full flex items-center justify-center bg-gradient-to-br ${
        kind === "favorites"
          ? "from-[#a238ff] via-[#ef5466] to-[#ff8b3d]"
          : "from-[#1f1c5c] via-[#6c2bd9] to-[#00c7f2]"
      }`}
    >
      {kind === "favorites" ? (
        <Heart size={size} className="text-white" fill="white" />
      ) : (
        <Waves size={size} className="text-white" />
      )}
    </div>
  );
}

interface DeezerTracksViewProps {
  kind: "favorites" | "flow";
}

/** Deezer Loved Tracks or Flow, as a playable track list. */
export default function DeezerTracksView({ kind }: DeezerTracksViewProps) {
  const { navigateHome } = useNavigation();
  const [favoriteIds, setFavoriteIds] = useAtom(deezerFavoriteTrackIdsAtom);
  const [tracks, setTracks] = useState<Track[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    (kind === "favorites" ? deezerGetFavoriteTracks() : deezerGetFlow())
      .then((t) => {
        if (cancelled) return;
        setTracks(t);
        // These are favourites by definition: make sure the hearts (and the
        // filter below) agree even if the startup id load is still pending.
        if (kind === "favorites")
          setFavoriteIds((prev) => new Set([...prev, ...t.map((x) => x.id)]));
      })
      .catch((err) => {
        if (!cancelled) setError(safeErrorMessage(err, "Failed to load"));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [kind, setFavoriteIds]);

  // Un-hearting a track here takes it off the list at once, like TIDAL's
  // Loved Tracks page; Flow is a stream and keeps everything.
  const shown =
    kind === "favorites" ? tracks.filter((t) => favoriteIds.has(t.id)) : tracks;

  const favorites = kind === "favorites";
  return (
    <DeezerCollectionPage
      label={favorites ? "Loved Tracks" : "Flow"}
      title={favorites ? "Loved Tracks" : "Flow"}
      subtitle={favorites ? undefined : "Your personal mix, endless and fresh"}
      artwork={<DeezerCollectionTile kind={kind} />}
      source={
        favorites
          ? {
              type: "deezer-favorites",
              id: "deezer-favorites",
              name: "Loved Tracks",
            }
          : { type: "deezer-flow", id: "deezer-flow", name: "Flow" }
      }
      tracks={shown}
      loading={loading}
      error={error}
      onBack={navigateHome}
    />
  );
}

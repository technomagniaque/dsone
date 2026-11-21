import { useEffect, useState } from "react";
import { Heart, RefreshCw, Sparkles, Waves } from "lucide-react";
import { useAtom } from "jotai";
import { useNavigation } from "../../hooks/useNavigation";
import { deezerGetFavoriteTracks, deezerGetFlow } from "../../api/deezer";
import {
  deezerFavoriteTrackIdsAtom,
  deezerFlowMoodAtom,
} from "../../atoms/deezer";
import { DEEZER_FLOW_MOODS, findFlowMood } from "../../lib/deezerFlowMoods";
import { safeErrorMessage } from "../../lib/errorUtils";
import type { Track } from "../../types";
import DeezerCollectionPage from "./DeezerCollectionPage";

/** Gradient tile used for Deezer's personal collections (no cover art). */
export function DeezerCollectionTile({
  kind,
  size = 56,
  gradient,
}: {
  kind: "favorites" | "flow";
  size?: number;
  gradient?: string;
}) {
  const grad =
    gradient ??
    (kind === "favorites"
      ? "from-[#a238ff] via-[#ef5466] to-[#ff8b3d]"
      : "from-[#1f1c5c] via-[#6c2bd9] to-[#00c7f2]");
  return (
    <div
      className={`w-full h-full flex items-center justify-center bg-gradient-to-br ${grad}`}
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

/** Deezer Loved Tracks or Flow (with Flow Moods & Genres), as a playable track list. */
export default function DeezerTracksView({ kind }: DeezerTracksViewProps) {
  const { navigateHome } = useNavigation();
  const [favoriteIds, setFavoriteIds] = useAtom(deezerFavoriteTrackIdsAtom);
  const [moodId, setMoodId] = useAtom(deezerFlowMoodAtom);
  const [refreshNonce, setRefreshNonce] = useState(0);
  const [tracks, setTracks] = useState<Track[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const activeMood = findFlowMood(moodId);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    (kind === "favorites"
      ? deezerGetFavoriteTracks()
      : deezerGetFlow(moodId))
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
  }, [kind, moodId, refreshNonce, setFavoriteIds]);

  // Un-hearting a track here takes it off the list at once, like TIDAL's
  // Loved Tracks page; Flow is a stream and keeps everything.
  const shown =
    kind === "favorites" ? tracks.filter((t) => favoriteIds.has(t.id)) : tracks;

  const favorites = kind === "favorites";
  const moods = DEEZER_FLOW_MOODS.filter((m) => m.category === "mood");
  const genres = DEEZER_FLOW_MOODS.filter((m) => m.category === "genre");

  const moodSelector = !favorites ? (
    <div className="mb-6 flex flex-col gap-3">
      <div>
        <div className="flex items-center gap-1.5 text-[11px] font-bold uppercase tracking-widest text-th-text-muted mb-2">
          <Sparkles size={12} className="text-th-accent" />
          Flow Moods
        </div>
        <div className="flex flex-wrap gap-2">
          {moods.map((m) => {
            const active = m.id === activeMood.id;
            return (
              <button
                key={m.id}
                type="button"
                onClick={() => setMoodId(m.id)}
                className={`px-3.5 py-1.5 rounded-full text-[12.5px] font-semibold transition-all border ${
                  active
                    ? "bg-th-accent text-th-on-accent border-transparent shadow-md"
                    : "bg-th-elevated/70 hover:bg-th-surface-hover text-th-text-primary border-th-border-subtle"
                }`}
              >
                {m.label}
              </button>
            );
          })}
        </div>
      </div>
      <div>
        <div className="text-[11px] font-bold uppercase tracking-widest text-th-text-muted mb-2">
          Flow by Genre
        </div>
        <div className="flex flex-wrap gap-1.5">
          {genres.map((g) => {
            const active = g.id === activeMood.id;
            return (
              <button
                key={g.id}
                type="button"
                onClick={() => setMoodId(g.id)}
                className={`px-3 py-1 rounded-full text-[11.5px] font-medium transition-all border ${
                  active
                    ? "bg-th-accent text-th-on-accent border-transparent"
                    : "bg-th-surface/70 hover:bg-th-surface-hover text-th-text-secondary hover:text-th-text-primary border-th-border-subtle"
                }`}
              >
                {g.label}
              </button>
            );
          })}
        </div>
      </div>
    </div>
  ) : undefined;

  return (
    <DeezerCollectionPage
      label={favorites ? "Loved Tracks" : "Flow · Deezer"}
      title={
        favorites
          ? "Loved Tracks"
          : activeMood.id === "default"
            ? "Flow"
            : `Flow · ${activeMood.label}`
      }
      subtitle={favorites ? undefined : activeMood.subtitle}
      artwork={
        <DeezerCollectionTile
          kind={kind}
          gradient={favorites ? undefined : activeMood.gradient}
        />
      }
      source={
        favorites
          ? {
              type: "deezer-favorites",
              id: "deezer-favorites",
              name: "Loved Tracks",
            }
          : {
              type: "deezer-flow",
              id:
                activeMood.id === "default"
                  ? "deezer-flow"
                  : `deezer-flow-${activeMood.id}`,
              name:
                activeMood.id === "default"
                  ? "Flow"
                  : `Flow · ${activeMood.label}`,
            }
      }
      tracks={shown}
      loading={loading}
      error={error}
      onBack={navigateHome}
      topSlot={moodSelector}
      headerActions={
        !favorites ? (
          <button
            type="button"
            onClick={() => setRefreshNonce((n) => n + 1)}
            className="flex flex-col items-center gap-1.5 text-th-text-muted hover:text-th-text-primary transition-colors"
            title="Load a fresh Flow batch"
            aria-label="Refresh Flow"
          >
            <RefreshCw size={20} />
            <span className="text-[11px] font-medium">Refresh</span>
          </button>
        ) : undefined
      }
    />
  );
}

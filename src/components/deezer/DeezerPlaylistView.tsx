import { useEffect, useState } from "react";
import { deezerGetPlaylist } from "../../api/deezer";
import { getDeezerShareUrl } from "../../lib/deezerSources";
import { safeErrorMessage } from "../../lib/errorUtils";
import type { Playlist, Track } from "../../types";
import DeezerCollectionPage from "./DeezerCollectionPage";

interface DeezerPlaylistViewProps {
  /** Deezer playlist id, as a string (the `uuid` of a Deezer Playlist). */
  playlistId: string;
  playlistInfo?: { title: string; image?: string; description?: string };
  onBack: () => void;
}

export default function DeezerPlaylistView({
  playlistId,
  playlistInfo,
  onBack,
}: DeezerPlaylistViewProps) {
  const [playlist, setPlaylist] = useState<Playlist | null>(null);
  const [tracks, setTracks] = useState<Track[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    deezerGetPlaylist(Number(playlistId))
      .then((r) => {
        if (cancelled) return;
        setPlaylist(r.playlist);
        setTracks(r.tracks);
      })
      .catch((err) => {
        if (!cancelled)
          setError(safeErrorMessage(err, "Failed to load playlist"));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [playlistId]);

  const title = playlist?.title || playlistInfo?.title || "Playlist";
  const image = playlist?.squareImage || playlist?.image || playlistInfo?.image;

  return (
    <DeezerCollectionPage
      label="Playlist"
      title={title}
      subtitle={playlist?.creator?.name}
      description={playlist?.description || playlistInfo?.description}
      image={image}
      source={{
        type: "deezer-playlist",
        id: playlistId,
        name: title,
        image,
      }}
      tracks={tracks}
      loading={loading}
      error={error}
      shareUrl={getDeezerShareUrl("playlist", playlistId)}
      onBack={onBack}
    />
  );
}

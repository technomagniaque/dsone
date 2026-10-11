import { useEffect, useState } from "react";
import { Music, Share, User } from "lucide-react";
import { usePlaySource } from "../../hooks/usePlaySource";
import { useNavigation } from "../../hooks/useNavigation";
import { useToast } from "../../contexts/ToastContext";
import {
  deezerGetArtist,
  type DeezerArtistPage as Page,
} from "../../api/deezer";
import { getDeezerShareUrl } from "../../lib/deezerSources";
import { useDeezerMediaPlay } from "../../hooks/useDeezerMediaPlay";
import { safeErrorMessage } from "../../lib/errorUtils";
import type { PlayableSource } from "../../lib/trackSources";
import CoverBanner from "../CoverBanner";
import TidalImage from "../TidalImage";
import TrackList from "../TrackList";
import MediaCard from "../MediaCard";
import MediaGrid from "../MediaGrid";
import PageContainer from "../PageContainer";
import SourcePlayButton from "../SourcePlayButton";
import { ArtistPageSkeleton } from "../PageSkeleton";

interface DeezerArtistPageProps {
  artistId: number;
  artistInfo?: { name: string; picture?: string };
  onBack: () => void;
}

const TOP_TRACKS_SHOWN = 10;

export default function DeezerArtistPage({
  artistId,
  artistInfo,
  onBack,
}: DeezerArtistPageProps) {
  const playSource = usePlaySource();
  const playDeezerMedia = useDeezerMediaPlay();
  const { navigateToAlbum } = useNavigation();
  const { showToast } = useToast();
  const [page, setPage] = useState<Page | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    deezerGetArtist(artistId)
      .then((p) => {
        if (!cancelled) setPage(p);
      })
      .catch((err) => {
        if (!cancelled)
          setError(safeErrorMessage(err, "Failed to load artist"));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [artistId]);

  if (loading) return <ArtistPageSkeleton />;

  if (error || !page) {
    return (
      <div className="flex-1 bg-linear-to-b from-th-surface to-th-base flex items-center justify-center">
        <div className="flex flex-col items-center gap-4 text-center px-8">
          <Music size={48} className="text-th-text-disabled" />
          <p className="text-th-text-primary font-semibold text-lg">
            Couldn't load artist
          </p>
          <p className="text-th-text-muted text-sm max-w-md">{error}</p>
          <button
            onClick={onBack}
            className="mt-2 px-6 py-2 bg-th-text-primary text-th-base rounded-full text-sm font-bold hover:scale-105 transition-transform"
          >
            Go back
          </button>
        </div>
      </div>
    );
  }

  const name = page.artist.name || artistInfo?.name || "Artist";
  const picture = page.artist.picture || artistInfo?.picture;
  const topTracks = page.topTracks;
  const playable = (): PlayableSource => ({
    meta: { type: "deezer-artist", id: artistId, name, image: picture },
    loaded: topTracks,
  });

  const handleShare = async () => {
    try {
      await navigator.clipboard.writeText(
        getDeezerShareUrl("artist", artistId),
      );
      showToast("Copied share link to clipboard");
    } catch {
      showToast("Failed to copy link", "error");
    }
  };

  return (
    <div className="flex-1 bg-linear-to-b from-th-surface to-th-base min-h-full">
      <div className="relative">
        <CoverBanner src={picture} />
        <PageContainer>
          <div className="px-8 pb-6 pt-10 flex items-end gap-7 relative z-10">
            <div className="w-[200px] h-[200px] shrink-0 rounded-full overflow-hidden shadow-[0_16px_48px_8px_rgba(0,0,0,0.55)] bg-th-surface-hover flex items-center justify-center">
              {picture ? (
                <TidalImage
                  src={picture}
                  alt={name}
                  type="artist"
                  className="w-full h-full"
                />
              ) : (
                <User size={64} className="text-th-text-faint" />
              )}
            </div>
            <div className="flex flex-col gap-2 pb-2 min-w-0">
              <span className="text-[12px] font-bold text-th-text-secondary uppercase tracking-widest">
                Artist · Deezer
              </span>
              <h1 className="text-[48px] font-extrabold text-th-text-primary leading-tight tracking-tight line-clamp-2">
                {name}
              </h1>
            </div>
          </div>
          <div className="px-8 py-4 flex items-center justify-between relative z-10">
            <SourcePlayButton
              sourceType="deezer-artist"
              sourceId={artistId}
              onPlay={() => void playSource(playable())}
            />
            <button
              onClick={handleShare}
              className="flex flex-col items-center gap-1.5 text-th-text-muted hover:text-th-text-primary transition-colors"
              title="Copy share link"
              aria-label="Share artist"
            >
              <Share size={22} />
              <span className="text-[11px] font-medium">Share</span>
            </button>
          </div>
        </PageContainer>
      </div>

      <PageContainer>
        {topTracks.length > 0 && (
          <section className="px-8 pt-4">
            <h2 className="text-[20px] font-bold text-th-text-primary mb-3">
              Top tracks
            </h2>
            <TrackList
              tracks={topTracks.slice(0, TOP_TRACKS_SHOWN)}
              onPlay={(track) =>
                void playSource(playable(), { startAt: track })
              }
              showDateAdded={false}
              showArtist={false}
              showAlbum={true}
              showCover={true}
              context="playlist"
            />
          </section>
        )}

        {page.albums.length > 0 && (
          <section className="px-8 pt-8 pb-10">
            <h2 className="text-[20px] font-bold text-th-text-primary mb-3">
              Albums
            </h2>
            <MediaGrid>
              {page.albums.map((album) => (
                <MediaCard
                  key={album.id}
                  item={album}
                  subtitleOverride={
                    album.releaseDate
                      ? String(new Date(album.releaseDate).getFullYear())
                      : undefined
                  }
                  onClick={() =>
                    navigateToAlbum(
                      album.id,
                      {
                        title: album.title,
                        cover: album.cover,
                        artistName: name,
                      },
                      "deezer",
                    )
                  }
                  onPlay={(e) => {
                    e.stopPropagation();
                    void playDeezerMedia("album", album.id, album.title);
                  }}
                />
              ))}
            </MediaGrid>
          </section>
        )}
      </PageContainer>
    </div>
  );
}

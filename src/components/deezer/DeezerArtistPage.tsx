import { useEffect, useState } from "react";
import {
  Loader2,
  Music,
  Play,
  Radio,
  Share,
  Shuffle,
  User,
} from "lucide-react";
import { usePlaySource } from "../../hooks/usePlaySource";
import { useNavigation } from "../../hooks/useNavigation";
import { useToast } from "../../contexts/ToastContext";
import {
  deezerGetArtist,
  type DeezerArtistPage as Page,
} from "../../api/deezer";
import {
  deezerArtistRadioPlayable,
  getDeezerShareUrl,
} from "../../lib/deezerSources";
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
  const { navigateToAlbum, navigateToArtist } = useNavigation();
  const { showToast } = useToast();
  const [page, setPage] = useState<Page | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showAllTracks, setShowAllTracks] = useState(false);
  const [radioLoading, setRadioLoading] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setShowAllTracks(false);
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

  const handleStartRadio = async () => {
    if (radioLoading) return;
    setRadioLoading(true);
    try {
      const src = await deezerArtistRadioPlayable(artistId, name, picture);
      await playSource(src);
      showToast(`Playing ${name} Radio`);
    } catch {
      showToast("Artist radio unavailable", "info");
    } finally {
      setRadioLoading(false);
    }
  };

  const kindOf = (a: { type?: string; albumType?: string }) =>
    a.type ?? a.albumType;
  const fullAlbums = page.albums.filter(
    (a) => kindOf(a) !== "EP" && kindOf(a) !== "SINGLE",
  );
  const epsAndSingles = page.albums.filter(
    (a) => kindOf(a) === "EP" || kindOf(a) === "SINGLE",
  );
  const relatedArtists = page.relatedArtists ?? [];
  const visibleTracks = showAllTracks
    ? topTracks
    : topTracks.slice(0, TOP_TRACKS_SHOWN);

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
            <div className="flex items-center gap-3">
              <SourcePlayButton
                sourceType="deezer-artist"
                sourceId={artistId}
                onPlay={() => void playSource(playable())}
              />
              <button
                onClick={() => void playSource(playable(), { shuffle: true })}
                className="flex items-center gap-2 px-6 py-2.5 bg-th-button/40 backdrop-blur-md text-th-text-primary font-bold text-sm rounded-full hover:bg-th-button/60 hover:scale-[1.03] transition-[transform,filter,background-color] duration-150"
              >
                <Shuffle size={18} />
                Shuffle
              </button>
              <button
                onClick={handleStartRadio}
                disabled={radioLoading}
                className="flex items-center gap-2 px-6 py-2.5 bg-th-button/40 backdrop-blur-md text-th-text-primary font-bold text-sm rounded-full hover:bg-th-button/60 hover:scale-[1.03] disabled:opacity-60 transition-[transform,filter,background-color] duration-150"
              >
                {radioLoading ? (
                  <Loader2 size={18} className="animate-spin" />
                ) : (
                  <Radio size={18} />
                )}
                Radio
              </button>
            </div>
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
            <div className="flex items-center justify-between mb-3">
              <h2 className="text-[20px] font-bold text-th-text-primary">
                Top tracks
              </h2>
              {topTracks.length > TOP_TRACKS_SHOWN && (
                <button
                  onClick={() => setShowAllTracks((v) => !v)}
                  className="text-xs font-semibold text-th-text-muted hover:text-th-text-primary transition-colors"
                >
                  {showAllTracks
                    ? "Show top 10"
                    : `Show all (${topTracks.length})`}
                </button>
              )}
            </div>
            <TrackList
              tracks={visibleTracks}
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

        {fullAlbums.length > 0 && (
          <section className="px-8 pt-8 pb-6">
            <h2 className="text-[20px] font-bold text-th-text-primary mb-3">
              Albums
            </h2>
            <MediaGrid>
              {fullAlbums.map((album) => (
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
                    void playDeezerMedia(
                      "album",
                      album.id,
                      album.title,
                      album.cover,
                    );
                  }}
                />
              ))}
            </MediaGrid>
          </section>
        )}

        {epsAndSingles.length > 0 && (
          <section className="px-8 pt-4 pb-6">
            <h2 className="text-[20px] font-bold text-th-text-primary mb-3">
              EPs &amp; Singles
            </h2>
            <MediaGrid>
              {epsAndSingles.map((album) => (
                <MediaCard
                  key={album.id}
                  item={album}
                  subtitleOverride={
                    album.releaseDate
                      ? `${kindOf(album) === "EP" ? "EP" : "Single"} · ${new Date(album.releaseDate).getFullYear()}`
                      : kindOf(album) === "EP"
                        ? "EP"
                        : "Single"
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
                    void playDeezerMedia(
                      "album",
                      album.id,
                      album.title,
                      album.cover,
                    );
                  }}
                />
              ))}
            </MediaGrid>
          </section>
        )}

        {relatedArtists.length > 0 && (
          <section className="px-8 pt-4 pb-8">
            <h2 className="text-[20px] font-bold text-th-text-primary mb-3">
              Fans also like
            </h2>
            <MediaGrid>
              {relatedArtists.map((rel) => (
                <div
                  key={rel.id}
                  onClick={() =>
                    navigateToArtist(
                      rel.id,
                      { name: rel.name, picture: rel.picture },
                      "deezer",
                    )
                  }
                  className="group bg-th-elevated/40 hover:bg-th-surface-hover p-4 rounded-xl cursor-pointer transition-all duration-300 flex flex-col items-center text-center"
                >
                  <div className="w-full aspect-square mb-3.5 relative rounded-full overflow-hidden bg-th-surface shadow-lg flex items-center justify-center">
                    {rel.picture ? (
                      <TidalImage
                        src={rel.picture}
                        alt={rel.name}
                        type="artist"
                        className="w-full h-full object-cover group-hover:scale-105 transition-transform duration-300"
                      />
                    ) : (
                      <User size={40} className="text-th-text-faint" />
                    )}
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        void playDeezerMedia(
                          "artist",
                          rel.id,
                          rel.name,
                          rel.picture,
                        );
                      }}
                      aria-label={`Play ${rel.name}`}
                      className="absolute bottom-2 right-2 w-10 h-10 bg-th-accent rounded-full flex items-center justify-center shadow-xl opacity-0 group-hover:opacity-100 transition-opacity hover:brightness-110"
                    >
                      <Play
                        size={18}
                        fill="currentColor"
                        className="text-th-on-accent ml-0.5"
                      />
                    </button>
                  </div>
                  <p className="text-[14px] font-bold text-th-text-primary truncate w-full">
                    {rel.name}
                  </p>
                  <p className="text-[12px] text-th-text-muted mt-0.5">
                    Artist
                  </p>
                </div>
              ))}
            </MediaGrid>
          </section>
        )}

        {page.bio && (
          <section className="px-8 pt-4 pb-10">
            <h2 className="text-[20px] font-bold text-th-text-primary mb-3">
              About
            </h2>
            <div className="rounded-xl bg-th-surface border border-th-border-subtle p-6 text-[14px] text-th-text-secondary leading-relaxed max-w-4xl whitespace-pre-line">
              {page.bio.replace(/<[^>]+>/g, "")}
            </div>
          </section>
        )}
      </PageContainer>
    </div>
  );
}

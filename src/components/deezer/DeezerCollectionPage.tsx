import type { ReactNode } from "react";
import { Music, Share, Shuffle } from "lucide-react";
import { useToast } from "../../contexts/ToastContext";
import { usePlaySource } from "../../hooks/usePlaySource";
import type { PlayableSource } from "../../lib/trackSources";
import { formatTotalDuration } from "../../utils/itemHelpers";
import type { Track } from "../../types";
import CoverBanner from "../CoverBanner";
import TidalImage from "../TidalImage";
import TrackList from "../TrackList";
import PageContainer from "../PageContainer";
import SourcePlayButton from "../SourcePlayButton";
import { DetailPageSkeleton } from "../PageSkeleton";

interface DeezerCollectionPageProps {
  /** Small caps label above the title ("Playlist", "Loved Tracks"…). */
  label: string;
  title: string;
  subtitle?: string;
  description?: string;
  /** Full image URL; ignored when `artwork` is given. */
  image?: string;
  /** Custom artwork (e.g. the Loved Tracks gradient tile). */
  artwork?: ReactNode;
  source: PlayableSource["meta"];
  tracks: Track[];
  loading: boolean;
  error: string | null;
  shareUrl?: string;
  onBack: () => void;
  showDateAdded?: boolean;
}

/** Header + track list shared by Deezer playlists, Loved Tracks and Flow.
 *  Same layout as the TIDAL playlist page, without TIDAL-only actions. */
export default function DeezerCollectionPage({
  label,
  title,
  subtitle,
  description,
  image,
  artwork,
  source,
  tracks,
  loading,
  error,
  shareUrl,
  onBack,
  showDateAdded = false,
}: DeezerCollectionPageProps) {
  const playSource = usePlaySource();
  const { showToast } = useToast();

  if (loading) return <DetailPageSkeleton type="playlist" />;

  if (error) {
    return (
      <div className="flex-1 bg-linear-to-b from-th-surface to-th-base flex items-center justify-center">
        <div className="flex flex-col items-center gap-4 text-center px-8">
          <Music size={48} className="text-th-text-disabled" />
          <p className="text-th-text-primary font-semibold text-lg">
            Couldn't load {label.toLowerCase()}
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

  const playable = (): PlayableSource => ({ meta: source, loaded: tracks });
  const totalDuration = tracks.reduce((sum, t) => sum + (t.duration || 0), 0);

  const handleShare = async () => {
    if (!shareUrl) return;
    try {
      await navigator.clipboard.writeText(shareUrl);
      showToast("Copied share link to clipboard");
    } catch {
      showToast("Failed to copy link", "error");
    }
  };

  return (
    <div className="flex-1 bg-linear-to-b from-th-surface to-th-base overflow-y-auto scrollbar-thin scrollbar-thumb-th-button scrollbar-track-transparent">
      <div className="relative">
        {!artwork && <CoverBanner src={image} />}
        <PageContainer>
          <div className="px-8 pb-8 pt-8 flex items-end gap-7 relative z-10">
            <div className="w-[232px] h-[232px] shrink-0 rounded-lg overflow-hidden shadow-[0_16px_48px_8px_rgba(0,0,0,0.55)] bg-th-surface-hover flex items-center justify-center">
              {artwork ??
                (image ? (
                  <TidalImage
                    src={image}
                    alt={title}
                    type="playlist"
                    className="w-full h-full"
                  />
                ) : (
                  <Music size={56} className="text-th-text-faint" />
                ))}
            </div>
            <div className="flex flex-col gap-2 pb-1 min-w-0">
              <span className="text-[12px] font-bold text-th-text-secondary uppercase tracking-widest">
                {label} · Deezer
              </span>
              <h1 className="text-[42px] font-extrabold text-th-text-primary leading-tight tracking-tight line-clamp-2">
                {title}
              </h1>
              {subtitle && (
                <div className="mt-2 text-[14px] text-th-text-primary font-semibold truncate">
                  {subtitle}
                </div>
              )}
              {description && (
                <p className="mt-1 max-w-[800px] text-[14px] text-th-text-muted line-clamp-2">
                  {description}
                </p>
              )}
              <div className="text-[12px] text-th-text-muted uppercase tracking-wide mt-2">
                <span>
                  {tracks.length} TRACK{tracks.length !== 1 ? "S" : ""}
                </span>
                {totalDuration > 0 && (
                  <span> ({formatTotalDuration(totalDuration)})</span>
                )}
              </div>
            </div>
          </div>

          <div className="px-8 py-5 flex items-center justify-between relative z-10">
            <div className="flex items-center gap-3">
              <SourcePlayButton
                sourceType={source.type}
                sourceId={source.id}
                onPlay={() => void playSource(playable())}
              />
              <button
                onClick={() => void playSource(playable(), { shuffle: true })}
                className="flex items-center gap-2 px-6 py-2.5 bg-th-button/40 backdrop-blur-md text-th-text-primary font-bold text-sm rounded-full hover:bg-th-button/60 hover:scale-[1.03] transition-[transform,filter,background-color] duration-150"
              >
                <Shuffle size={18} />
                Shuffle
              </button>
            </div>
            {shareUrl && (
              <button
                onClick={handleShare}
                className="flex flex-col items-center gap-1.5 text-th-text-muted hover:text-th-text-primary transition-colors"
                title="Copy share link"
                aria-label={`Share ${label.toLowerCase()}`}
              >
                <Share size={22} />
                <span className="text-[11px] font-medium">Share</span>
              </button>
            )}
          </div>
        </PageContainer>
      </div>

      <PageContainer>
        <div className="px-8 pt-4 pb-8">
          {tracks.length === 0 ? (
            <p className="text-th-text-muted text-sm py-8 text-center">
              Nothing here yet
            </p>
          ) : (
            <TrackList
              tracks={tracks}
              onPlay={(track) =>
                void playSource(playable(), { startAt: track })
              }
              showDateAdded={showDateAdded}
              showArtist={true}
              showAlbum={true}
              showCover={true}
              context="playlist"
            />
          )}
        </div>
      </PageContainer>
    </div>
  );
}

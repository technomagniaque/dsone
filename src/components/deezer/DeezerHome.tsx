import { useEffect, useState } from "react";
import { useAtomValue } from "jotai";
import { Play } from "lucide-react";
import { deezerUserAtom } from "../../atoms/deezer";
import { deezerGetUserAlbums, deezerGetUserPlaylists } from "../../api/deezer";
import { useNavigation } from "../../hooks/useNavigation";
import { usePlaySource } from "../../hooks/usePlaySource";
import { useDeezerMediaPlay } from "../../hooks/useDeezerMediaPlay";
import { useToast } from "../../contexts/ToastContext";
import {
  deezerFavoritesPlayable,
  deezerFlowPlayable,
} from "../../lib/deezerSources";
import type { PlayableSource } from "../../lib/trackSources";
import type { AlbumDetail, Playlist } from "../../types";
import MediaCard from "../MediaCard";
import MediaGrid, { MediaGridSkeleton } from "../MediaGrid";
import PageContainer from "../PageContainer";
import { DeezerCollectionTile } from "./DeezerTracksView";

function greeting(): string {
  const h = new Date().getHours();
  if (h < 5) return "Good night";
  if (h < 12) return "Good morning";
  if (h < 18) return "Good afternoon";
  return "Good evening";
}

/** Deezer home: Flow, Loved Tracks, the user's playlists and albums. */
export default function DeezerHome() {
  const user = useAtomValue(deezerUserAtom);
  const { navigateTo, navigateToPlaylist, navigateToAlbum } = useNavigation();
  const playSource = usePlaySource();
  const playDeezerMedia = useDeezerMediaPlay();
  const { showToast } = useToast();
  const [playlists, setPlaylists] = useState<Playlist[] | null>(null);
  const [albums, setAlbums] = useState<AlbumDetail[] | null>(null);

  useEffect(() => {
    let cancelled = false;
    deezerGetUserPlaylists()
      .then((p) => !cancelled && setPlaylists(p))
      .catch((err) => {
        console.error("Failed to load Deezer playlists:", err);
        if (!cancelled) setPlaylists([]);
      });
    deezerGetUserAlbums()
      .then((a) => !cancelled && setAlbums(a))
      .catch((err) => {
        console.error("Failed to load Deezer albums:", err);
        if (!cancelled) setAlbums([]);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const playCollection = (load: () => Promise<PlayableSource>) => {
    load()
      .then((src) => playSource(src))
      .catch(() => showToast("Couldn't load tracks", "error"));
  };

  const tiles = [
    {
      kind: "flow" as const,
      title: "Flow",
      subtitle: "Your endless personal mix",
      open: () => navigateTo({ type: "deezerFlow" }),
      play: () => playCollection(deezerFlowPlayable),
    },
    {
      kind: "favorites" as const,
      title: "Loved Tracks",
      subtitle: "Your favourite tracks on Deezer",
      open: () => navigateTo({ type: "deezerFavorites" }),
      play: () => playCollection(deezerFavoritesPlayable),
    },
  ];

  return (
    <div className="flex-1 bg-linear-to-b from-th-surface to-th-base min-h-full">
      <PageContainer className="px-8 py-8">
        <p className="text-[12px] font-bold text-th-text-secondary uppercase tracking-widest">
          Deezer
        </p>
        <h1 className="text-[32px] font-extrabold text-th-text-primary tracking-tight mb-6">
          {greeting()}
          {user?.name ? `, ${user.name}` : ""}
        </h1>

        <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 mb-10">
          {tiles.map((t) => (
            <div
              key={t.kind}
              onClick={t.open}
              className="group flex items-center gap-4 bg-th-elevated hover:bg-th-surface-hover rounded-lg overflow-hidden cursor-pointer transition-colors"
            >
              <div className="w-20 h-20 shrink-0">
                <DeezerCollectionTile kind={t.kind} size={30} />
              </div>
              <div className="flex-1 min-w-0">
                <p className="text-[16px] font-bold text-th-text-primary truncate">
                  {t.title}
                </p>
                <p className="text-[12.5px] text-th-text-muted truncate">
                  {t.subtitle}
                </p>
              </div>
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  t.play();
                }}
                aria-label={`Play ${t.title}`}
                className="mr-4 w-11 h-11 bg-th-accent rounded-full flex items-center justify-center shadow-xl opacity-0 group-hover:opacity-100 transition-opacity hover:brightness-110 hover:scale-105"
              >
                <Play
                  size={20}
                  fill="currentColor"
                  className="text-th-on-accent ml-0.5"
                />
              </button>
            </div>
          ))}
        </div>

        <section className="mb-10">
          <h2 className="text-[20px] font-bold text-th-text-primary mb-3">
            Your playlists
          </h2>
          {playlists === null ? (
            <MediaGridSkeleton count={6} />
          ) : playlists.length === 0 ? (
            <p className="text-th-text-muted text-sm">No playlists yet</p>
          ) : (
            <MediaGrid>
              {playlists.map((pl) => (
                <MediaCard
                  key={pl.uuid}
                  item={pl}
                  onClick={() =>
                    navigateToPlaylist(
                      pl.uuid,
                      {
                        title: pl.title,
                        image: pl.squareImage || pl.image,
                        description: pl.description,
                      },
                      "deezer",
                    )
                  }
                  onPlay={(e) => {
                    e.stopPropagation();
                    void playDeezerMedia(
                      "playlist",
                      pl.uuid,
                      pl.title,
                      pl.squareImage || pl.image,
                    );
                  }}
                />
              ))}
            </MediaGrid>
          )}
        </section>

        <section className="mb-10">
          <h2 className="text-[20px] font-bold text-th-text-primary mb-3">
            Your albums
          </h2>
          {albums === null ? (
            <MediaGridSkeleton count={6} />
          ) : albums.length === 0 ? (
            <p className="text-th-text-muted text-sm">No albums yet</p>
          ) : (
            <MediaGrid>
              {albums.map((album) => (
                <MediaCard
                  key={album.id}
                  item={album}
                  onClick={() =>
                    navigateToAlbum(
                      album.id,
                      {
                        title: album.title,
                        cover: album.cover,
                        artistName: album.artist?.name,
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
          )}
        </section>
      </PageContainer>
    </div>
  );
}

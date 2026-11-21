import { useEffect, useState } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { Play, Plus, User } from "lucide-react";
import { deezerPlaylistsVersionAtom, deezerUserAtom } from "../../atoms/deezer";
import {
  deezerGetCharts,
  deezerGetUserAlbums,
  deezerGetUserArtists,
  deezerGetUserPlaylists,
  type DeezerArtist,
} from "../../api/deezer";
import { useNavigation } from "../../hooks/useNavigation";
import { usePlaySource } from "../../hooks/usePlaySource";
import { useDeezerMediaPlay } from "../../hooks/useDeezerMediaPlay";
import { useToast } from "../../contexts/ToastContext";
import {
  deezerFavoritesPlayable,
  deezerFlowPlayable,
} from "../../lib/deezerSources";
import type { PlayableSource } from "../../lib/trackSources";
import {
  getTidalImageUrl,
  type AlbumDetail,
  type Playlist,
  type SearchResults,
} from "../../types";
import { CreatePlaylistModal } from "../AddToPlaylistMenu";
import MediaCard from "../MediaCard";
import MediaGrid, { MediaGridSkeleton } from "../MediaGrid";
import PageContainer from "../PageContainer";
import TidalImage from "../TidalImage";
import TrackList from "../TrackList";
import { DeezerCollectionTile } from "./DeezerTracksView";

function greeting(): string {
  const h = new Date().getHours();
  if (h < 5) return "Good night";
  if (h < 12) return "Good morning";
  if (h < 18) return "Good afternoon";
  return "Good evening";
}

/** Deezer home: Flow, Loved Tracks, the user's playlists, albums, artists, and Deezer Charts. */
export default function DeezerHome() {
  const user = useAtomValue(deezerUserAtom);
  const playlistsVersion = useAtomValue(deezerPlaylistsVersionAtom);
  const bumpPlaylists = useSetAtom(deezerPlaylistsVersionAtom);
  const { navigateTo, navigateToPlaylist, navigateToAlbum, navigateToArtist } =
    useNavigation();
  const playSource = usePlaySource();
  const playDeezerMedia = useDeezerMediaPlay();
  const { showToast } = useToast();
  const [playlists, setPlaylists] = useState<Playlist[] | null>(null);
  const [albums, setAlbums] = useState<AlbumDetail[] | null>(null);
  const [artists, setArtists] = useState<DeezerArtist[] | null>(null);
  const [charts, setCharts] = useState<SearchResults | null>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const [playlistsLimit, setPlaylistsLimit] = useState(12);
  const [albumsLimit, setAlbumsLimit] = useState(12);
  const [artistsLimit, setArtistsLimit] = useState(12);

  useEffect(() => {
    let cancelled = false;
    deezerGetUserPlaylists()
      .then((p) => !cancelled && setPlaylists(p))
      .catch((err) => {
        console.error("Failed to load Deezer playlists:", err);
        if (!cancelled) setPlaylists([]);
      });
    return () => {
      cancelled = true;
    };
  }, [playlistsVersion]);

  useEffect(() => {
    let cancelled = false;
    deezerGetUserAlbums()
      .then((a) => !cancelled && setAlbums(a))
      .catch((err) => {
        console.error("Failed to load Deezer albums:", err);
        if (!cancelled) setAlbums([]);
      });
    deezerGetUserArtists()
      .then((a) => !cancelled && setArtists(a))
      .catch((err) => {
        console.error("Failed to load Deezer artists:", err);
        if (!cancelled) setArtists([]);
      });
    deezerGetCharts(12)
      .then((c) => !cancelled && setCharts(c))
      .catch((err) => {
        console.error("Failed to load Deezer charts:", err);
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
          <div className="flex items-center justify-between mb-3">
            <h2 className="text-[20px] font-bold text-th-text-primary">
              Your playlists
              {playlists && playlists.length > 0 && (
                <span className="ml-2 text-xs font-normal text-th-text-muted">
                  ({playlists.length})
                </span>
              )}
            </h2>
            <div className="flex items-center gap-2">
              {playlists && playlists.length > 12 && (
                <button
                  onClick={() =>
                    setPlaylistsLimit((n) =>
                      n >= playlists.length ? 12 : Math.min(playlists.length, n + 24),
                    )
                  }
                  className="text-xs font-semibold text-th-text-secondary hover:text-th-text-primary transition-colors"
                >
                  {playlistsLimit >= playlists.length ? "Show less" : "Show more"}
                </button>
              )}
              <button
                onClick={() => setCreateOpen(true)}
                className="flex items-center gap-1.5 px-3.5 py-1.5 rounded-full bg-th-elevated hover:bg-th-surface-hover border border-th-border-subtle text-xs font-semibold text-th-text-primary transition-colors"
              >
                <Plus size={14} />
                New playlist
              </button>
            </div>
          </div>
          {playlists === null ? (
            <MediaGridSkeleton count={6} />
          ) : playlists.length === 0 ? (
            <p className="text-th-text-muted text-sm">No playlists yet</p>
          ) : (
            <MediaGrid>
              {playlists.slice(0, playlistsLimit).map((pl) => (
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
          <div className="flex items-center justify-between mb-3">
            <h2 className="text-[20px] font-bold text-th-text-primary">
              Your albums
              {albums && albums.length > 0 && (
                <span className="ml-2 text-xs font-normal text-th-text-muted">
                  ({albums.length})
                </span>
              )}
            </h2>
            {albums && albums.length > 12 && (
              <button
                onClick={() =>
                  setAlbumsLimit((n) =>
                    n >= albums.length ? 12 : Math.min(albums.length, n + 24),
                  )
                }
                className="text-xs font-semibold text-th-text-secondary hover:text-th-text-primary transition-colors"
              >
                {albumsLimit >= albums.length ? "Show less" : "Show more"}
              </button>
            )}
          </div>
          {albums === null ? (
            <MediaGridSkeleton count={6} />
          ) : albums.length === 0 ? (
            <p className="text-th-text-muted text-sm">No albums yet</p>
          ) : (
            <MediaGrid>
              {albums.slice(0, albumsLimit).map((album) => (
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

        {artists && artists.length > 0 && (
          <section className="mb-10">
            <div className="flex items-center justify-between mb-3">
              <h2 className="text-[20px] font-bold text-th-text-primary">
                Your artists
                <span className="ml-2 text-xs font-normal text-th-text-muted">
                  ({artists.length})
                </span>
              </h2>
              {artists.length > 12 && (
                <button
                  onClick={() =>
                    setArtistsLimit((n) =>
                      n >= artists.length ? 12 : Math.min(artists.length, n + 24),
                    )
                  }
                  className="text-xs font-semibold text-th-text-secondary hover:text-th-text-primary transition-colors"
                >
                  {artistsLimit >= artists.length ? "Show less" : "Show more"}
                </button>
              )}
            </div>
            <MediaGrid>
              {artists.slice(0, artistsLimit).map((artist) => (
                <div
                  key={artist.id}
                  onClick={() =>
                    navigateToArtist(
                      artist.id,
                      { name: artist.name, picture: artist.picture },
                      "deezer",
                    )
                  }
                  className="group bg-th-elevated/40 hover:bg-th-surface-hover p-4 rounded-xl cursor-pointer transition-all duration-300 flex flex-col items-center text-center"
                >
                  <div className="w-full aspect-square mb-3.5 relative rounded-full overflow-hidden bg-th-surface shadow-lg flex items-center justify-center">
                    {artist.picture ? (
                      <TidalImage
                        src={getTidalImageUrl(artist.picture, 320)}
                        alt={artist.name}
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
                          artist.id,
                          artist.name,
                          artist.picture,
                        );
                      }}
                      aria-label={`Play ${artist.name}`}
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
                    {artist.name}
                  </p>
                  <p className="text-[12px] text-th-text-muted mt-0.5">
                    Artist
                  </p>
                </div>
              ))}
            </MediaGrid>
          </section>
        )}

        {charts && charts.tracks.length > 0 && (
          <section className="mb-10">
            <h2 className="text-[20px] font-bold text-th-text-primary mb-3">
              Deezer Top Tracks
            </h2>
            <TrackList
              tracks={charts.tracks.slice(0, 10)}
              onPlay={(track) =>
                void playSource(
                  {
                    meta: {
                      type: "deezer-charts",
                      id: "deezer-charts",
                      name: "Deezer Top Tracks",
                    },
                    loaded: charts.tracks,
                  },
                  { startAt: track },
                )
              }
              showDateAdded={false}
              showArtist={true}
              showAlbum={true}
              showCover={true}
              context="playlist"
            />
          </section>
        )}

        {charts && charts.playlists.length > 0 && (
          <section className="mb-10">
            <h2 className="text-[20px] font-bold text-th-text-primary mb-3">
              Top Deezer Playlists
            </h2>
            <MediaGrid>
              {charts.playlists.map((pl) => (
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
          </section>
        )}

        {charts && charts.albums.length > 0 && (
          <section className="mb-10">
            <h2 className="text-[20px] font-bold text-th-text-primary mb-3">
              Top Deezer Albums
            </h2>
            <MediaGrid>
              {charts.albums.map((album) => (
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
          </section>
        )}

        {createOpen && (
          <CreatePlaylistModal
            trackIds={[]}
            provider="deezer"
            onClose={() => setCreateOpen(false)}
            onCreated={(pl) => {
              setCreateOpen(false);
              bumpPlaylists((v) => v + 1);
              navigateToPlaylist(
                pl.uuid,
                { title: pl.title, description: pl.description },
                "deezer",
              );
            }}
          />
        )}
      </PageContainer>
    </div>
  );
}

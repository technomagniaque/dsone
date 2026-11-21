import { useEffect, useState } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { Music, Plus, User } from "lucide-react";
import { currentViewAtom } from "../../atoms/navigation";
import { deezerPlaylistsVersionAtom } from "../../atoms/deezer";
import {
  deezerGetUserAlbums,
  deezerGetUserArtists,
  deezerGetUserPlaylists,
  type DeezerArtist,
} from "../../api/deezer";
import { useNavigation } from "../../hooks/useNavigation";
import { getTidalImageUrl, type AlbumDetail, type Playlist } from "../../types";
import { CreatePlaylistModal } from "../AddToPlaylistMenu";
import TidalImage from "../TidalImage";
import SidebarSkeleton from "../SidebarSkeleton";
import { DeezerCollectionTile } from "./DeezerTracksView";

interface DeezerSidebarSectionProps {
  collapsed: boolean;
  /** Deezer-only session: the section takes the library's place and also
   *  lists the user's playlists, albums and artists. */
  fill: boolean;
}

type DeezerLibraryTab = "playlists" | "albums" | "artists";

/** Sidebar entries for the Deezer account (Home, Flow, Loved Tracks and,
 *  when it is the only service, the library tabs: playlists, albums, artists). */
export default function DeezerSidebarSection({
  collapsed,
  fill,
}: DeezerSidebarSectionProps) {
  const currentView = useAtomValue(currentViewAtom);
  const { navigateTo, navigateToPlaylist, navigateToAlbum, navigateToArtist } =
    useNavigation();
  const [activeTab, setActiveTab] = useState<DeezerLibraryTab>("playlists");
  const [playlists, setPlaylists] = useState<Playlist[] | null>(null);
  const [albums, setAlbums] = useState<AlbumDetail[] | null>(null);
  const [artists, setArtists] = useState<DeezerArtist[] | null>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const playlistsVersion = useAtomValue(deezerPlaylistsVersionAtom);
  const bumpPlaylists = useSetAtom(deezerPlaylistsVersionAtom);

  useEffect(() => {
    if (!fill) return;
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
  }, [fill, playlistsVersion]);

  useEffect(() => {
    if (!fill || activeTab !== "albums" || albums !== null) return;
    let cancelled = false;
    deezerGetUserAlbums()
      .then((a) => !cancelled && setAlbums(a))
      .catch((err) => {
        console.error("Failed to load Deezer albums:", err);
        if (!cancelled) setAlbums([]);
      });
    return () => {
      cancelled = true;
    };
  }, [fill, activeTab, albums]);

  useEffect(() => {
    if (!fill || activeTab !== "artists" || artists !== null) return;
    let cancelled = false;
    deezerGetUserArtists()
      .then((a) => !cancelled && setArtists(a))
      .catch((err) => {
        console.error("Failed to load Deezer artists:", err);
        if (!cancelled) setArtists([]);
      });
    return () => {
      cancelled = true;
    };
  }, [fill, activeTab, artists]);

  const rowClass = (active: boolean) =>
    `w-full flex items-center gap-2.5 px-1.5 py-2 rounded-md transition-colors duration-150 ${
      active ? "bg-th-hl-med" : "hover:bg-th-border-subtle"
    } ${collapsed ? "justify-center" : ""}`;

  const entries = [
    {
      key: "deezerFlow" as const,
      kind: "flow" as const,
      title: "Flow",
      subtitle: "Deezer mix",
    },
    {
      key: "deezerFavorites" as const,
      kind: "favorites" as const,
      title: "Loved Tracks",
      subtitle: "Deezer",
    },
  ];

  return (
    <div className={fill ? "flex-1 flex flex-col min-h-0" : "shrink-0"}>
      {!collapsed && (
        <div className="px-4 pt-3 pb-1 flex items-center justify-between">
          <button
            onClick={() => navigateTo({ type: "deezerHome" })}
            className={`text-[11px] font-bold tracking-[1.2px] uppercase transition-colors ${
              currentView.type === "deezerHome"
                ? "text-th-accent"
                : "text-th-text-faint hover:text-th-text-primary"
            }`}
            title="Deezer home"
          >
            Deezer
          </button>
          <button
            onClick={() => setCreateOpen(true)}
            className="p-1 rounded-full text-th-text-faint hover:text-th-text-primary hover:bg-th-border-subtle transition-colors"
            title="Create Deezer playlist"
            aria-label="Create Deezer playlist"
          >
            <Plus size={14} />
          </button>
        </div>
      )}

      {fill && !collapsed && (
        <div className="px-2 pb-2 pt-1 flex gap-1.5 overflow-x-auto no-scrollbar">
          {(["playlists", "albums", "artists"] as const).map((tab) => (
            <button
              key={tab}
              onClick={() => setActiveTab(tab)}
              className={`px-2.5 py-1 rounded-full text-xs font-medium whitespace-nowrap transition-colors duration-150 ${
                activeTab === tab
                  ? "bg-th-accent/15 text-th-accent"
                  : "bg-th-hl-med hover:bg-th-inset text-th-text-secondary"
              }`}
            >
              {tab === "playlists"
                ? "Playlists"
                : tab === "albums"
                  ? "Albums"
                  : "Artists"}
            </button>
          ))}
        </div>
      )}

      <div
        className={`px-1.5 pb-2 space-y-px ${fill ? "flex-1 overflow-y-auto custom-scrollbar" : ""}`}
      >
        {(!fill || activeTab === "playlists") &&
          entries.map((e) => (
            <button
              key={e.key}
              onClick={() => navigateTo({ type: e.key })}
              className={rowClass(currentView.type === e.key)}
              title={`${e.title} (Deezer)`}
            >
              <div className="w-10 h-10 shrink-0 rounded overflow-hidden">
                <DeezerCollectionTile kind={e.kind} size={15} />
              </div>
              {!collapsed && (
                <div className="flex-1 min-w-0 text-left">
                  <div className="text-[13px] font-medium text-th-text-primary truncate leading-tight">
                    {e.title}
                  </div>
                  <div className="text-[12px] text-th-text-faint truncate leading-snug mt-0.5">
                    {e.subtitle}
                  </div>
                </div>
              )}
            </button>
          ))}

        {fill &&
          activeTab === "playlists" &&
          (playlists === null ? (
            <SidebarSkeleton count={4} />
          ) : (
            playlists.map((pl) => {
              const image = pl.squareImage || pl.image;
              const active =
                currentView.type === "playlist" &&
                currentView.provider === "deezer" &&
                currentView.playlistId === pl.uuid;
              return (
                <button
                  key={pl.uuid}
                  onClick={() =>
                    navigateToPlaylist(
                      pl.uuid,
                      {
                        title: pl.title,
                        image,
                        description: pl.description,
                      },
                      "deezer",
                    )
                  }
                  className={rowClass(active)}
                  title={pl.title}
                >
                  <div className="w-10 h-10 shrink-0 rounded overflow-hidden bg-th-surface-hover flex items-center justify-center">
                    {image ? (
                      <TidalImage
                        src={getTidalImageUrl(image, 80)}
                        alt={pl.title}
                        type="playlist"
                        className="w-full h-full"
                      />
                    ) : (
                      <Music size={16} className="text-th-text-faint" />
                    )}
                  </div>
                  {!collapsed && (
                    <div className="flex-1 min-w-0 text-left">
                      <div className="text-[13px] font-medium text-th-text-primary truncate leading-tight">
                        {pl.title}
                      </div>
                      <div className="text-[12px] text-th-text-faint truncate leading-snug mt-0.5">
                        {pl.numberOfTracks != null
                          ? `${pl.numberOfTracks} track${pl.numberOfTracks !== 1 ? "s" : ""}`
                          : "Playlist"}
                      </div>
                    </div>
                  )}
                </button>
              );
            })
          ))}

        {fill &&
          activeTab === "albums" &&
          (albums === null ? (
            <SidebarSkeleton count={4} />
          ) : albums.length === 0 ? (
            !collapsed && (
              <p className="px-3 py-6 text-center text-th-text-muted text-xs">
                No saved albums on Deezer
              </p>
            )
          ) : (
            albums.map((album) => {
              const active =
                currentView.type === "album" &&
                currentView.provider === "deezer" &&
                currentView.albumId === album.id;
              return (
                <button
                  key={album.id}
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
                  className={rowClass(active)}
                  title={album.title}
                >
                  <div className="w-10 h-10 shrink-0 rounded overflow-hidden bg-th-surface-hover flex items-center justify-center">
                    {album.cover ? (
                      <TidalImage
                        src={getTidalImageUrl(album.cover, 80)}
                        alt={album.title}
                        type="album"
                        className="w-full h-full"
                      />
                    ) : (
                      <Music size={16} className="text-th-text-faint" />
                    )}
                  </div>
                  {!collapsed && (
                    <div className="flex-1 min-w-0 text-left">
                      <div className="text-[13px] font-medium text-th-text-primary truncate leading-tight">
                        {album.title}
                      </div>
                      <div className="text-[12px] text-th-text-faint truncate leading-snug mt-0.5">
                        {album.artist?.name || "Album"}
                      </div>
                    </div>
                  )}
                </button>
              );
            })
          ))}

        {fill &&
          activeTab === "artists" &&
          (artists === null ? (
            <SidebarSkeleton count={4} />
          ) : artists.length === 0 ? (
            !collapsed && (
              <p className="px-3 py-6 text-center text-th-text-muted text-xs">
                No followed artists on Deezer
              </p>
            )
          ) : (
            artists.map((artist) => {
              const active =
                currentView.type === "artist" &&
                currentView.provider === "deezer" &&
                currentView.artistId === artist.id;
              return (
                <button
                  key={artist.id}
                  onClick={() =>
                    navigateToArtist(
                      artist.id,
                      { name: artist.name, picture: artist.picture },
                      "deezer",
                    )
                  }
                  className={rowClass(active)}
                  title={artist.name}
                >
                  <div className="w-10 h-10 shrink-0 rounded-full overflow-hidden bg-th-surface-hover flex items-center justify-center">
                    {artist.picture ? (
                      <TidalImage
                        src={getTidalImageUrl(artist.picture, 80)}
                        alt={artist.name}
                        type="artist"
                        className="w-full h-full"
                      />
                    ) : (
                      <User size={16} className="text-th-text-faint" />
                    )}
                  </div>
                  {!collapsed && (
                    <div className="flex-1 min-w-0 text-left">
                      <div className="text-[13px] font-medium text-th-text-primary truncate leading-tight">
                        {artist.name}
                      </div>
                      <div className="text-[12px] text-th-text-faint truncate leading-snug mt-0.5">
                        Artist
                      </div>
                    </div>
                  )}
                </button>
              );
            })
          ))}
      </div>

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
    </div>
  );
}

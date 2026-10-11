import { useEffect, useState } from "react";
import { useAtomValue } from "jotai";
import { Music } from "lucide-react";
import { currentViewAtom } from "../../atoms/navigation";
import { deezerGetUserPlaylists } from "../../api/deezer";
import { useNavigation } from "../../hooks/useNavigation";
import type { Playlist } from "../../types";
import TidalImage from "../TidalImage";
import SidebarSkeleton from "../SidebarSkeleton";
import { DeezerCollectionTile } from "./DeezerTracksView";

interface DeezerSidebarSectionProps {
  collapsed: boolean;
  /** Deezer-only session: the section takes the library's place and also
   *  lists the user's playlists. */
  fill: boolean;
}

/** Sidebar entries for the Deezer account (Home, Flow, Loved Tracks and,
 *  when it is the only service, the playlists). */
export default function DeezerSidebarSection({
  collapsed,
  fill,
}: DeezerSidebarSectionProps) {
  const currentView = useAtomValue(currentViewAtom);
  const { navigateTo, navigateToPlaylist } = useNavigation();
  const [playlists, setPlaylists] = useState<Playlist[] | null>(null);

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
  }, [fill]);

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
        </div>
      )}
      <div
        className={`px-1.5 pb-2 space-y-px ${fill ? "flex-1 overflow-y-auto custom-scrollbar" : ""}`}
      >
        {entries.map((e) => (
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
                        src={image}
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
      </div>
    </div>
  );
}

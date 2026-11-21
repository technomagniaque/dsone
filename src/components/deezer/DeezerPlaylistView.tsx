import { useEffect, useState } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { Loader2, Pencil, Trash2, X } from "lucide-react";
import {
  deezerDeletePlaylist,
  deezerGetPlaylist,
  deezerUpdatePlaylist,
} from "../../api/deezer";
import { deezerPlaylistsVersionAtom, deezerUserAtom } from "../../atoms/deezer";
import { useToast } from "../../contexts/ToastContext";
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
  const user = useAtomValue(deezerUserAtom);
  const bumpPlaylists = useSetAtom(deezerPlaylistsVersionAtom);
  const { showToast } = useToast();
  const [playlist, setPlaylist] = useState<Playlist | null>(null);
  const [tracks, setTracks] = useState<Track[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [editOpen, setEditOpen] = useState(false);
  const [editTitle, setEditTitle] = useState("");
  const [editDesc, setEditDesc] = useState("");
  const [editPublic, setEditPublic] = useState(true);
  const [saving, setSaving] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [deleting, setDeleting] = useState(false);

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
  const description = playlist?.description || playlistInfo?.description;
  const image = playlist?.squareImage || playlist?.image || playlistInfo?.image;
  const isUserPlaylist =
    !!user && !!playlist?.creator?.id && playlist.creator.id === user.id;

  const openEdit = () => {
    setEditTitle(title);
    setEditDesc(description || "");
    setEditPublic(playlist?.accessType !== "UNLISTED");
    setEditOpen(true);
  };

  const handleSaveEdit = async () => {
    if (!editTitle.trim() || saving) return;
    setSaving(true);
    try {
      await deezerUpdatePlaylist(
        Number(playlistId),
        editTitle.trim(),
        editDesc.trim(),
        editPublic,
      );
      setPlaylist((prev) =>
        prev
          ? {
              ...prev,
              title: editTitle.trim(),
              description: editDesc.trim(),
              accessType: editPublic ? "PUBLIC" : "UNLISTED",
            }
          : prev,
      );
      bumpPlaylists((v) => v + 1);
      setEditOpen(false);
      showToast("Updated playlist");
    } catch (err) {
      showToast(safeErrorMessage(err, "Failed to update playlist"), "error");
    } finally {
      setSaving(false);
    }
  };

  const handleDeletePlaylist = async () => {
    if (deleting) return;
    setDeleting(true);
    try {
      await deezerDeletePlaylist(Number(playlistId));
      bumpPlaylists((v) => v + 1);
      showToast(`Deleted playlist "${title}"`);
      setConfirmDelete(false);
      onBack();
    } catch (err) {
      showToast(safeErrorMessage(err, "Failed to delete playlist"), "error");
      setDeleting(false);
    }
  };

  return (
    <>
      <DeezerCollectionPage
        label="Playlist"
        title={title}
        subtitle={playlist?.creator?.name}
        description={description}
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
        playlistId={playlistId}
        isUserPlaylist={isUserPlaylist}
        onTrackRemoved={(idx) =>
          setTracks((prev) => prev.filter((_, i) => i !== idx))
        }
        headerActions={
          isUserPlaylist ? (
            <>
              <button
                onClick={openEdit}
                className="flex flex-col items-center gap-1.5 text-th-text-muted hover:text-th-text-primary transition-colors"
                title="Edit playlist"
                aria-label="Edit playlist"
              >
                <Pencil size={20} />
                <span className="text-[11px] font-medium">Edit</span>
              </button>
              <button
                onClick={() => setConfirmDelete(true)}
                className="flex flex-col items-center gap-1.5 text-th-text-muted hover:text-red-400 transition-colors"
                title="Delete playlist"
                aria-label="Delete playlist"
              >
                <Trash2 size={20} />
                <span className="text-[11px] font-medium">Delete</span>
              </button>
            </>
          ) : undefined
        }
      />

      {editOpen && (
        <div
          className="fixed inset-0 z-[10000] flex items-center justify-center bg-black/60 backdrop-blur-xs"
          onClick={() => !saving && setEditOpen(false)}
        >
          <div
            className="w-[460px] max-w-[90vw] bg-th-elevated rounded-2xl border border-th-border-subtle shadow-2xl overflow-hidden"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center justify-between px-6 pt-5 pb-3">
              <h2 className="text-[18px] font-bold text-th-text-primary">
                Edit playlist
              </h2>
              <button
                onClick={() => setEditOpen(false)}
                disabled={saving}
                className="p-1.5 rounded-full hover:bg-th-hl-med text-th-text-muted hover:text-th-text-primary transition-colors"
              >
                <X size={18} />
              </button>
            </div>
            <div className="px-6 py-4 flex flex-col gap-4">
              <input
                type="text"
                value={editTitle}
                onChange={(e) => setEditTitle(e.target.value)}
                placeholder="Playlist title"
                disabled={saving}
                className="w-full px-4 py-3 bg-th-surface border border-th-border-subtle rounded-lg text-[14px] text-th-text-primary placeholder:text-th-text-faint focus:outline-none focus:border-th-accent transition-colors"
              />
              <textarea
                value={editDesc}
                onChange={(e) => setEditDesc(e.target.value)}
                placeholder="Description (optional)"
                rows={3}
                disabled={saving}
                className="w-full px-4 py-3 bg-th-surface border border-th-border-subtle rounded-lg text-[14px] text-th-text-primary placeholder:text-th-text-faint focus:outline-none focus:border-th-accent transition-colors resize-none"
              />
              <label className="flex items-center justify-between text-[13.5px] text-th-text-primary cursor-pointer">
                <span>Public playlist</span>
                <input
                  type="checkbox"
                  checked={editPublic}
                  onChange={(e) => setEditPublic(e.target.checked)}
                  disabled={saving}
                  className="accent-th-accent w-4 h-4"
                />
              </label>
            </div>
            <div className="flex justify-end gap-2 px-6 pt-2 pb-6">
              <button
                onClick={() => setEditOpen(false)}
                disabled={saving}
                className="px-5 py-2 text-[13.5px] font-semibold text-th-text-muted hover:text-th-text-primary rounded-full transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={handleSaveEdit}
                disabled={!editTitle.trim() || saving}
                className="px-6 py-2 bg-th-accent text-th-on-accent text-[13.5px] font-semibold rounded-full hover:brightness-110 disabled:opacity-40 transition-all flex items-center gap-2"
              >
                {saving && <Loader2 size={15} className="animate-spin" />}
                Save
              </button>
            </div>
          </div>
        </div>
      )}

      {confirmDelete && (
        <div
          className="fixed inset-0 z-[10000] flex items-center justify-center bg-black/60 backdrop-blur-xs"
          onClick={() => !deleting && setConfirmDelete(false)}
        >
          <div
            className="w-[400px] max-w-[90vw] bg-th-elevated rounded-2xl border border-th-border-subtle shadow-2xl p-6"
            onClick={(e) => e.stopPropagation()}
          >
            <h2 className="text-[17px] font-bold text-th-text-primary mb-2">
              Delete playlist?
            </h2>
            <p className="text-[13.5px] text-th-text-muted mb-6">
              Are you sure you want to delete &ldquo;{title}&rdquo; from your
              Deezer account?
            </p>
            <div className="flex justify-end gap-2">
              <button
                onClick={() => setConfirmDelete(false)}
                disabled={deleting}
                className="px-5 py-2 text-[13.5px] font-semibold text-th-text-muted hover:text-th-text-primary rounded-full transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={handleDeletePlaylist}
                disabled={deleting}
                className="px-5 py-2 bg-red-500 text-white text-[13.5px] font-semibold rounded-full hover:bg-red-600 disabled:opacity-50 transition-colors flex items-center gap-2"
              >
                {deleting && <Loader2 size={15} className="animate-spin" />}
                Delete
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}

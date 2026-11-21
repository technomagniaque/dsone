import { useCallback, useEffect, useRef } from "react";
import { useAtom, useAtomValue, useStore } from "jotai";
import {
  deezerCloudQueueSyncAtom,
  deezerQueueSyncStatusAtom,
  deezerUserAtom,
} from "../atoms/deezer";
import {
  contextSourceAtom,
  currentTrackAtom,
  historyAtom,
  isPlayingAtom,
  manualQueueAtom,
  originalQueueAtom,
  playbackSourceAtom,
  queueAtom,
  queueEpochAtom,
  repeatAtom,
  shuffleAtom,
  streamInfoAtom,
} from "../atoms/playback";
import {
  deezerGetHistory,
  deezerPullCloudQueue,
  deezerPushCloudQueue,
  type DeezerCloudQueueState,
} from "../api/deezer";
import { useToast } from "../contexts/ToastContext";
import { ensureQid } from "../lib/qid";
import { isDeezer } from "../lib/provider";
import type { Track } from "../types";

type JotaiStore = ReturnType<typeof useStore>;

/** Fingerprint of the active Deezer queue state to avoid redundant cloud pushes
 *  and prevent echoing back a state we just pulled from another device. */
export function buildQueueSignature(
  history: Track[],
  current: Track | null,
  upcoming: Track[],
  repeat: number,
  shuffle: boolean,
): string | null {
  if (!current || !isDeezer(current)) return null;
  const full = [...history, current, ...upcoming].filter(isDeezer);
  const pos = full.findIndex((t) => t._qid === current._qid || t.id === current.id);
  const ids = full.map((t) => t.id).join(",");
  return `${pos}:${repeat}:${shuffle ? 1 : 0}:${ids}`;
}

/** Extract the ordered Deezer track IDs and current index from SONE's split
 *  `history + currentTrack + queue` representation. */
export function collectDeezerQueue(
  history: Track[],
  current: Track | null,
  upcoming: Track[],
): { trackIds: number[]; position: number } | null {
  if (!current || !isDeezer(current)) return null;
  const before = history.filter(isDeezer);
  const after = upcoming.filter(isDeezer);
  // Keep at most 50 recent history items before the active track so the synced
  // window stays focused around the active track and upcoming items.
  const trimmedBefore = before.length > 50 ? before.slice(before.length - 50) : before;
  const trackIds = [
    ...trimmedBefore.map((t) => t.id),
    current.id,
    ...after.map((t) => t.id),
  ];
  return { trackIds, position: trimmedBefore.length };
}

/** Apply a pulled Deezer Cloud Queue state into SONE's Jotai store (paused,
 *  ready to resume at the exact synced track). */
export function applyDeezerCloudQueue(
  store: JotaiStore,
  cloud: DeezerCloudQueueState,
): Track | null {
  if (!cloud.tracks.length) return null;
  const stamped = cloud.tracks.map((t) => ensureQid({ ...t, provider: "deezer" }));
  const pos = Math.max(0, Math.min(cloud.position, stamped.length - 1));
  const before = stamped.slice(0, pos);
  const active = stamped[pos];
  const after = stamped.slice(pos + 1);

  store.set(historyAtom, before);
  store.set(currentTrackAtom, active);
  store.set(queueAtom, after);
  store.set(manualQueueAtom, []);
  store.set(originalQueueAtom, null);
  store.set(repeatAtom, cloud.repeatMode);
  store.set(shuffleAtom, cloud.shuffled);
  store.set(streamInfoAtom, null);
  const source = {
    type: "deezer-cloud-sync",
    id: cloud.localQueueId ?? "deezer-cloud",
    name: "Deezer Cloud Sync",
    tracks: stamped,
  };
  store.set(playbackSourceAtom, source);
  store.set(contextSourceAtom, source);
  store.set(queueEpochAtom, (e) => e + 1);
  store.set(deezerQueueSyncStatusAtom, {
    version: cloud.version,
    localQueueId: cloud.localQueueId ?? null,
    lastSyncedAt: Date.now(),
    syncing: false,
    lastSource: cloud.sourceUserAgent ?? "Deezer",
  });
  return active;
}

/** Manual sync actions & state for DeezerTab, DeezerHome and NowPlayingDrawer. */
export function useDeezerQueueSyncActions() {
  const store = useStore();
  const user = useAtomValue(deezerUserAtom);
  const [enabled, setEnabled] = useAtom(deezerCloudQueueSyncAtom);
  const [status, setStatus] = useAtom(deezerQueueSyncStatusAtom);
  const { showToast } = useToast();

  const pullNow = useCallback(
    async (opts?: { silent?: boolean; allowHistoryFallback?: boolean }) => {
      if (!user) return false;
      setStatus((prev) => ({ ...prev, syncing: true }));
      try {
        const cloud = await deezerPullCloudQueue();
        if (cloud && cloud.tracks.length > 0) {
          const active = applyDeezerCloudQueue(store, cloud);
          if (!opts?.silent && active) {
            showToast(`Synced from Deezer: ${active.title}`);
          }
          return true;
        }
        if (opts?.allowHistoryFallback) {
          const historyTracks = await deezerGetHistory(30);
          if (historyTracks.length > 0) {
            const active = applyDeezerCloudQueue(store, {
              tracks: historyTracks,
              position: 0,
              repeatMode: 0,
              shuffled: false,
              version: "",
              sourceUserAgent: "Deezer History",
            });
            if (!opts?.silent && active) {
              showToast(`Loaded recent Deezer playback: ${active.title}`);
            }
            return true;
          }
        }
        setStatus((prev) => ({ ...prev, syncing: false }));
        if (!opts?.silent) {
          showToast("No active queue found on Deezer", "info");
        }
        return false;
      } catch (err) {
        console.warn("[deezer-sync] pull failed:", err);
        setStatus((prev) => ({ ...prev, syncing: false }));
        if (!opts?.silent) {
          showToast("Couldn't sync queue from Deezer", "error");
        }
        return false;
      }
    },
    [user, store, setStatus, showToast],
  );

  const pushNow = useCallback(
    async (opts?: { silent?: boolean }) => {
      if (!user) return false;
      const collected = collectDeezerQueue(
        store.get(historyAtom),
        store.get(currentTrackAtom),
        store.get(queueAtom),
      );
      if (!collected) {
        if (!opts?.silent) {
          showToast("Play a Deezer track first to sync to other devices", "info");
        }
        return false;
      }
      setStatus((prev) => ({ ...prev, syncing: true }));
      try {
        const prev = store.get(deezerQueueSyncStatusAtom);
        const src = store.get(playbackSourceAtom);
        const res = await deezerPushCloudQueue({
          trackIds: collected.trackIds,
          position: collected.position,
          repeatMode: store.get(repeatAtom),
          shuffled: store.get(shuffleAtom),
          previousVersion: prev.version ?? undefined,
          localQueueId: prev.localQueueId ?? undefined,
          contextId: src?.id != null ? String(src.id) : undefined,
          contextType: src?.type,
        });
        setStatus({
          version: res.version || prev.version,
          localQueueId: res.localQueueId,
          lastSyncedAt: Date.now(),
          syncing: false,
          lastSource: "dsone",
        });
        if (!opts?.silent) {
          showToast("Queue synced to Deezer devices");
        }
        return true;
      } catch (err) {
        console.warn("[deezer-sync] push failed:", err);
        setStatus((prev) => ({ ...prev, syncing: false }));
        if (!opts?.silent) {
          showToast("Couldn't push queue to Deezer", "error");
        }
        return false;
      }
    },
    [user, store, setStatus, showToast],
  );

  return { enabled, setEnabled, status, pullNow, pushNow };
}

/** Background bidirectional Deezer Cloud Queue sync (mounted once in AppInitializer). */
export function useDeezerQueueSync() {
  const store = useStore();
  const user = useAtomValue(deezerUserAtom);
  const enabled = useAtomValue(deezerCloudQueueSyncAtom);
  const currentTrack = useAtomValue(currentTrackAtom);
  const queue = useAtomValue(queueAtom);
  const history = useAtomValue(historyAtom);
  const repeat = useAtomValue(repeatAtom);
  const shuffle = useAtomValue(shuffleAtom);
  const { showToast } = useToast();

  // Signature last synced with the cloud so we never re-push what we just pulled.
  const lastSyncedSigRef = useRef<string | null>(null);
  const lastPullCheckRef = useRef<number>(0);
  const initialPulledUserRef = useRef<number | null>(null);

  const checkCloudForUpdates = useCallback(
    async (reason: "startup" | "focus") => {
      if (!user || !enabled) return;
      // Never hijack an actively playing session in dsone.
      if (store.get(isPlayingAtom)) return;
      const now = Date.now();
      if (reason === "focus" && now - lastPullCheckRef.current < 15_000) return;
      lastPullCheckRef.current = now;

      try {
        const cloud = await deezerPullCloudQueue();
        if (!cloud || !cloud.tracks.length) return;
        const prev = store.get(deezerQueueSyncStatusAtom);
        // Ignore if this exact cloud version is already loaded (or was pushed by us).
        if (prev.version && cloud.version && prev.version === cloud.version) {
          return;
        }
        const active = applyDeezerCloudQueue(store, cloud);
        if (active) {
          lastSyncedSigRef.current = buildQueueSignature(
            store.get(historyAtom),
            store.get(currentTrackAtom),
            store.get(queueAtom),
            store.get(repeatAtom),
            store.get(shuffleAtom),
          );
          showToast(`Synced from Deezer: ${active.title}`);
        }
      } catch (err) {
        console.debug("[deezer-sync] background pull skipped:", err);
      }
    },
    [user, enabled, store, showToast],
  );

  // Pull once shortly after login/startup so dsone picks up Android playback.
  useEffect(() => {
    if (!user || !enabled) return;
    if (initialPulledUserRef.current === user.id) return;
    initialPulledUserRef.current = user.id;
    const timer = window.setTimeout(() => {
      void checkCloudForUpdates("startup");
    }, 900);
    return () => window.clearTimeout(timer);
  }, [user, enabled, checkCloudForUpdates]);

  // Also check when returning to the dsone window while paused.
  useEffect(() => {
    if (!user || !enabled) return;
    const onFocus = () => {
      void checkCloudForUpdates("focus");
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [user, enabled, checkCloudForUpdates]);

  // Debounced push whenever the user changes Deezer track / queue / repeat / shuffle.
  useEffect(() => {
    if (!user || !enabled) return;
    const sig = buildQueueSignature(history, currentTrack, queue, repeat, shuffle);
    if (!sig || sig === lastSyncedSigRef.current) return;

    const timer = window.setTimeout(() => {
      const collected = collectDeezerQueue(
        store.get(historyAtom),
        store.get(currentTrackAtom),
        store.get(queueAtom),
      );
      if (!collected) return;
      const latestSig = buildQueueSignature(
        store.get(historyAtom),
        store.get(currentTrackAtom),
        store.get(queueAtom),
        store.get(repeatAtom),
        store.get(shuffleAtom),
      );
      if (!latestSig || latestSig === lastSyncedSigRef.current) return;

      const prev = store.get(deezerQueueSyncStatusAtom);
      const src = store.get(playbackSourceAtom);
      lastSyncedSigRef.current = latestSig;
      deezerPushCloudQueue({
        trackIds: collected.trackIds,
        position: collected.position,
        repeatMode: store.get(repeatAtom),
        shuffled: store.get(shuffleAtom),
        previousVersion: prev.version ?? undefined,
        localQueueId: prev.localQueueId ?? undefined,
        contextId: src?.id != null ? String(src.id) : undefined,
        contextType: src?.type,
      })
        .then((res) => {
          store.set(deezerQueueSyncStatusAtom, {
            version: res.version || prev.version,
            localQueueId: res.localQueueId,
            lastSyncedAt: Date.now(),
            syncing: false,
            lastSource: "dsone",
          });
        })
        .catch((err) => {
          // Allow retry on the next state change.
          lastSyncedSigRef.current = null;
          console.debug("[deezer-sync] background push failed:", err);
        });
    }, 1500);

    return () => window.clearTimeout(timer);
  }, [user, enabled, currentTrack, queue, history, repeat, shuffle, store]);
}

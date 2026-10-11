import { useEffect, useState } from "react";
import { useAtom, useAtomValue } from "jotai";
import { LogOut, User } from "lucide-react";
import {
  deezerQualityAtom,
  deezerUserAtom,
  type DeezerQuality,
} from "../../atoms/deezer";
import { getDeezerQuality, setDeezerQuality } from "../../api/deezer";
import { useDeezer } from "../../hooks/useDeezer";
import { useToast } from "../../contexts/ToastContext";
import TidalImage from "../TidalImage";
import DeezerLoginForm from "../deezer/DeezerLoginForm";
import { TierPicker } from "./QualityPicker";

const DEEZER_TIERS: readonly {
  id: DeezerQuality;
  label: string;
  spec: string;
}[] = [
  { id: "FLAC", label: "Lossless", spec: "FLAC · 16-bit / 44.1 kHz" },
  { id: "MP3_320", label: "High", spec: "MP3 · 320 kbps" },
  { id: "MP3_128", label: "Standard", spec: "MP3 · 128 kbps" },
];

const sectionLabel =
  "text-[10.5px] font-bold tracking-[1.4px] uppercase text-th-text-faint mb-2.5";

export default function DeezerTab() {
  const user = useAtomValue(deezerUserAtom);
  const [quality, setQuality] = useAtom(deezerQualityAtom);
  const { logout } = useDeezer();
  const { showToast } = useToast();
  const [loggingOut, setLoggingOut] = useState(false);

  // Backend-authoritative: refresh on open in case it changed elsewhere.
  useEffect(() => {
    getDeezerQuality()
      .then(setQuality)
      .catch(() => {});
  }, [setQuality]);

  const pick = (next: DeezerQuality) => {
    const prev = quality;
    setQuality(next);
    setDeezerQuality(next).catch(() => {
      setQuality(prev);
      showToast("Couldn't change Deezer quality", "error");
    });
  };

  if (!user) {
    return (
      <div>
        <p className={sectionLabel}>Deezer account</p>
        <div className="rounded-[14px] bg-th-surface border border-th-border-subtle p-5">
          <DeezerLoginForm
            onSuccess={(u) => showToast(`Signed in to Deezer as ${u.name}`)}
          />
        </div>
      </div>
    );
  }

  const plan = user.canStreamLossless
    ? "Lossless streaming available"
    : user.canStreamHq
      ? "Up to MP3 320 kbps on this plan"
      : "MP3 128 kbps on this plan";

  return (
    <div>
      <p className={sectionLabel}>Deezer account</p>
      <div className="rounded-[14px] bg-th-surface border border-th-border-subtle flex items-center gap-3.5 px-4 py-3">
        <div className="w-11 h-11 rounded-full bg-th-button overflow-hidden shrink-0 flex items-center justify-center">
          {user.picture ? (
            <TidalImage
              src={user.picture}
              alt=""
              type="artist"
              className="w-full h-full"
            />
          ) : (
            <User size={18} className="text-th-text-faint" />
          )}
        </div>
        <div className="flex-1 min-w-0">
          <p className="text-[13.5px] font-semibold text-th-text-primary truncate">
            {user.name}
          </p>
          <p className="text-[11.5px] text-th-text-muted mt-0.5">
            {plan}
            {user.country ? ` · ${user.country}` : ""}
          </p>
        </div>
        <button
          disabled={loggingOut}
          onClick={async () => {
            setLoggingOut(true);
            await logout();
            setLoggingOut(false);
          }}
          className="flex items-center gap-2 px-3.5 py-1.5 rounded-full text-[12.5px] font-semibold text-red-400 hover:bg-th-hl-faint transition-colors disabled:opacity-50"
        >
          <LogOut size={14} />
          Log out
        </button>
      </div>

      <p className={`${sectionLabel} mt-6`}>Audio quality</p>
      <TierPicker tiers={DEEZER_TIERS} value={quality} onSelect={pick} />
      <p className="text-[11px] text-th-text-muted mt-2.5">
        Quality requested from Deezer. When a track or your plan doesn't offer
        it, playback steps down to the next tier and lets you know.
      </p>
    </div>
  );
}

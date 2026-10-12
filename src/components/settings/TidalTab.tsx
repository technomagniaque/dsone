import { useState } from "react";
import { useAtomValue } from "jotai";
import { LogIn, LogOut, User } from "lucide-react";
import { currentUserAvatarAtom, userNameAtom } from "../../atoms/auth";
import { useAuth } from "../../hooks/useAuth";
import { useNavigation } from "../../hooks/useNavigation";
import TidalImage from "../TidalImage";

const sectionLabel =
  "text-[10.5px] font-bold tracking-[1.4px] uppercase text-th-text-faint mb-2.5";

/** TIDAL account: optional next to Deezer. Signing in opens the full TIDAL
 *  sign-in screen (browser, login code or custom credentials). */
export default function TidalTab({ onClose }: { onClose: () => void }) {
  const { isAuthenticated, logout } = useAuth();
  const userName = useAtomValue(userNameAtom);
  const avatar = useAtomValue(currentUserAvatarAtom);
  const { navigateTo } = useNavigation();
  const [loggingOut, setLoggingOut] = useState(false);

  if (!isAuthenticated) {
    return (
      <div>
        <p className={sectionLabel}>TIDAL account</p>
        <div className="rounded-[14px] bg-th-surface border border-th-border-subtle p-5 flex flex-col gap-4">
          <p className="text-[12.5px] text-th-text-muted leading-relaxed">
            TIDAL is optional. Sign in to add its catalogue, Hi-Res Lossless
            streams and your TIDAL library next to Deezer.
          </p>
          <button
            onClick={() => {
              onClose();
              navigateTo({ type: "tidalLogin" });
            }}
            className="self-start flex items-center gap-2 px-5 py-2.5 bg-th-accent text-th-on-accent font-bold rounded-full hover:brightness-110 transition-all text-[14px]"
          >
            <LogIn size={15} />
            Sign in to TIDAL
          </button>
        </div>
      </div>
    );
  }

  return (
    <div>
      <p className={sectionLabel}>TIDAL account</p>
      <div className="rounded-[14px] bg-th-surface border border-th-border-subtle flex items-center gap-3.5 px-4 py-3">
        <div className="w-11 h-11 rounded-full bg-th-button overflow-hidden shrink-0 flex items-center justify-center">
          {avatar ? (
            <TidalImage
              src={avatar}
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
            {userName}
          </p>
          <p className="text-[11.5px] text-th-text-muted mt-0.5">
            Streaming quality is set in Playback.
          </p>
        </div>
        <button
          disabled={loggingOut}
          onClick={async () => {
            setLoggingOut(true);
            try {
              await logout();
            } finally {
              setLoggingOut(false);
            }
          }}
          className="flex items-center gap-2 px-3.5 py-1.5 rounded-full text-[12.5px] font-semibold text-red-400 hover:bg-th-hl-faint transition-colors disabled:opacity-50"
        >
          <LogOut size={14} />
          Log out
        </button>
      </div>
    </div>
  );
}

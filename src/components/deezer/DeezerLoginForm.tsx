import { useState } from "react";
import { useAtomValue } from "jotai";
import {
  ChevronDown,
  ChevronRight,
  ExternalLink,
  Eye,
  EyeOff,
  Loader2,
} from "lucide-react";
import { useDeezer } from "../../hooks/useDeezer";
import { useOpenDeezerSignIn } from "../../hooks/useDeezerBrowserLogin";
import { deezerBrowserLoginAtom, type DeezerUser } from "../../atoms/deezer";
import { deezerLoginErrorMessage } from "../../lib/deezerLogin";

interface DeezerLoginFormProps {
  onSuccess?: (user: DeezerUser) => void;
  onCancel?: () => void;
  /** Label of the `onCancel` link (default "Back"). */
  cancelLabel?: string;
}

/** Deezer sign-in, shared by the login screen and Settings: Deezer's own
 *  page in the browser (like Deezer Enhanced), or a pasted ARL. */
export default function DeezerLoginForm({
  onSuccess,
  onCancel,
  cancelLabel = "Back",
}: DeezerLoginFormProps) {
  const { login } = useDeezer();
  const openSignIn = useOpenDeezerSignIn();
  const browser = useAtomValue(deezerBrowserLoginAtom);
  const [showArl, setShowArl] = useState(false);
  const [arl, setArl] = useState("");
  const [show, setShow] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    const value = arl.trim();
    if (!value || pending) return;
    setPending(true);
    setError(null);
    try {
      const user = await login(value);
      setArl("");
      onSuccess?.(user);
    } catch (err) {
      setError(deezerLoginErrorMessage(err));
    } finally {
      setPending(false);
    }
  };

  return (
    <div className="flex flex-col gap-4 text-left">
      <button
        type="button"
        onClick={() => {
          setError(null);
          void openSignIn();
        }}
        disabled={browser.state === "signing-in"}
        className="w-full flex items-center justify-center gap-2 px-6 py-3.5 bg-th-accent text-th-on-accent font-bold rounded-full hover:brightness-110 transition-all text-[16px] shadow-xl disabled:opacity-50 disabled:cursor-not-allowed"
      >
        {browser.state === "signing-in" ? (
          <Loader2 size={16} className="animate-spin" />
        ) : (
          <ExternalLink size={16} />
        )}
        {browser.state === "signing-in" ? "Signing in…" : "Sign in with Deezer"}
      </button>

      {browser.state === "waiting" && (
        <p
          role="status"
          className="flex items-start gap-2 text-[12.5px] text-th-text-muted leading-relaxed"
        >
          <Loader2 size={14} className="animate-spin mt-0.5 shrink-0" />
          Finish signing in on the Deezer page that just opened in your browser.
          When Deezer asks, allow it to open dsone: you will be signed in here
          automatically.
        </p>
      )}
      {browser.state === "error" && (
        <p
          role="alert"
          className="p-3 bg-red-900/30 border border-red-700/50 rounded-lg text-red-400 text-[12.5px] break-words"
        >
          {browser.message}
        </p>
      )}

      <p className="text-center text-[12px] text-th-text-faint">
        No account?{" "}
        <button
          type="button"
          onClick={() => void openSignIn(true)}
          className="underline hover:text-th-text-muted transition-colors"
        >
          Create one on Deezer
        </button>
      </p>

      <button
        type="button"
        onClick={() => setShowArl((v) => !v)}
        aria-expanded={showArl}
        className="inline-flex items-center gap-1 self-start text-[12px] text-th-text-faint hover:text-th-text-muted transition-colors"
      >
        {showArl ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
        Paste an ARL token instead
      </button>

      {showArl && (
        <form onSubmit={submit} className="flex flex-col gap-4">
          <label className="flex flex-col gap-1.5">
            <span className="text-[12px] font-semibold text-th-text-secondary">
              ARL token
            </span>
            <div className="relative">
              <input
                type={show ? "text" : "password"}
                value={arl}
                onChange={(e) => setArl(e.target.value)}
                placeholder="Paste your Deezer ARL"
                autoComplete="off"
                spellCheck={false}
                aria-label="ARL token"
                className="w-full px-3.5 py-2.5 pr-10 bg-th-overlay border border-th-border-subtle rounded-lg text-[13px] text-th-text-primary placeholder:text-th-text-faint font-mono focus:outline-none focus:border-th-accent"
              />
              <button
                type="button"
                onClick={() => setShow((v) => !v)}
                className="absolute right-2.5 top-1/2 -translate-y-1/2 text-th-text-faint hover:text-th-text-muted"
                aria-label={show ? "Hide ARL" : "Show ARL"}
              >
                {show ? <EyeOff size={15} /> : <Eye size={15} />}
              </button>
            </div>
          </label>

          <div className="text-[11.5px] text-th-text-muted leading-relaxed bg-th-overlay/60 rounded-lg px-3.5 py-3 border border-th-border-subtle">
            <p className="font-semibold text-th-text-secondary mb-1">
              Where to find it
            </p>
            <ol className="list-decimal pl-4 space-y-0.5">
              <li>Sign in on deezer.com in your browser.</li>
              <li>
                Open the developer tools (F12) → Storage (Firefox) or
                Application (Chrome) → Cookies → https://www.deezer.com.
              </li>
              <li>
                Copy the value of the <code className="font-mono">arl</code>{" "}
                cookie and paste it here.
              </li>
            </ol>
            <p className="mt-1.5 text-th-text-faint">
              The ARL gives full access to your account: keep it private. It is
              stored encrypted on this device.
            </p>
          </div>

          {error && (
            <p
              role="alert"
              className="p-3 bg-red-900/30 border border-red-700/50 rounded-lg text-red-400 text-[12.5px] break-words"
            >
              {error}
            </p>
          )}

          <button
            type="submit"
            disabled={!arl.trim() || pending || browser.state === "signing-in"}
            className="w-full flex items-center justify-center gap-2 px-6 py-3 bg-th-overlay border border-th-border-subtle text-th-text-primary font-bold rounded-full hover:bg-th-hl-faint transition-colors text-[15px] disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {pending && <Loader2 size={16} className="animate-spin" />}
            {pending ? "Signing in…" : "Sign in with ARL"}
          </button>
        </form>
      )}

      {onCancel && (
        <button
          type="button"
          onClick={onCancel}
          className="text-[12px] text-th-text-faint hover:text-th-text-muted transition-colors"
        >
          {cancelLabel}
        </button>
      )}
    </div>
  );
}

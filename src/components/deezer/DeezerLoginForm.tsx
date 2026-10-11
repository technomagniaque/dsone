import { useState } from "react";
import { Eye, EyeOff, Loader2 } from "lucide-react";
import { useDeezer } from "../../hooks/useDeezer";
import type { DeezerUser } from "../../atoms/deezer";

/** Pull a readable message out of whatever the backend rejected with. */
function loginErrorMessage(err: unknown): string {
  if (typeof err === "string") {
    try {
      const parsed = JSON.parse(err) as { message?: unknown };
      if (typeof parsed?.message === "string") return parsed.message;
    } catch {
      return err;
    }
  }
  if (err && typeof err === "object") {
    const msg = (err as { message?: unknown }).message;
    if (typeof msg === "string") return msg;
  }
  return "Sign-in failed — check the ARL and try again";
}

interface DeezerLoginFormProps {
  onSuccess?: (user: DeezerUser) => void;
  onCancel?: () => void;
}

/** ARL sign-in for Deezer, shared by the login screen and Settings. */
export default function DeezerLoginForm({
  onSuccess,
  onCancel,
}: DeezerLoginFormProps) {
  const { login } = useDeezer();
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
      setError(loginErrorMessage(err));
    } finally {
      setPending(false);
    }
  };

  return (
    <form onSubmit={submit} className="flex flex-col gap-4 text-left">
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
            Open the developer tools (F12) → Storage (Firefox) or Application
            (Chrome) → Cookies → https://www.deezer.com.
          </li>
          <li>
            Copy the value of the <code className="font-mono">arl</code> cookie
            and paste it here.
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
        disabled={!arl.trim() || pending}
        className="w-full flex items-center justify-center gap-2 px-6 py-3 bg-th-accent text-th-on-accent font-bold rounded-full hover:brightness-110 transition-all text-[15px] disabled:opacity-50 disabled:cursor-not-allowed"
      >
        {pending && <Loader2 size={16} className="animate-spin" />}
        {pending ? "Signing in…" : "Sign in with Deezer"}
      </button>
      {onCancel && (
        <button
          type="button"
          onClick={onCancel}
          className="text-[12px] text-th-text-faint hover:text-th-text-muted transition-colors"
        >
          Back
        </button>
      )}
    </form>
  );
}

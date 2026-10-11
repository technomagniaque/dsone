/** Deezer's own desktop sign-up page (the one Deezer Enhanced opens).
 *  Afterwards Deezer redirects the browser to `deezer://autolog/<ARL>`,
 *  which the desktop file registers dsone to handle. Sign-in itself uses
 *  an app window (`start_deezer_login_window`). */
const CALLBACK =
  "redirect_uri=https://www.deezer.com/desktop/login/electron/callback?source=electron";

export const DEEZER_SIGNUP_URL = `https://account.deezer.com/en/signup/?source=electron&${CALLBACK}`;

const AUTOLOG_PREFIX = "deezer://autolog/";

/** The ARL carried by a `deezer://autolog/<ARL>` link, or null. */
export function parseDeezerAutologUrl(url: string): string | null {
  if (!url.toLowerCase().startsWith(AUTOLOG_PREFIX)) return null;
  const arl = url.slice(AUTOLOG_PREFIX.length).split(/[?#/]/, 1)[0].trim();
  // Same shape the backend accepts (deezer::validate_arl).
  return /^[A-Za-z0-9\-_.%]{32,1024}$/.test(arl) ? arl : null;
}

/** Pull a readable message out of whatever the backend rejected with. */
export function deezerLoginErrorMessage(err: unknown): string {
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

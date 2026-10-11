import { useCallback, useEffect, useRef } from "react";
import { useSetAtom } from "jotai";
import { getCurrent, onOpenUrl } from "@tauri-apps/plugin-deep-link";
import { openUrl } from "@tauri-apps/plugin-opener";
import { deezerBrowserLoginAtom } from "../atoms/deezer";
import {
  DEEZER_LOGIN_URL,
  DEEZER_SIGNUP_URL,
  deezerLoginErrorMessage,
  parseDeezerAutologUrl,
} from "../lib/deezerLogin";
import { useDeezer } from "./useDeezer";

/** Open Deezer's own sign-in (or sign-up) page in the default browser. */
export function useOpenDeezerSignIn() {
  const setState = useSetAtom(deezerBrowserLoginAtom);
  return useCallback(
    async (signup = false) => {
      setState({ state: "waiting" });
      try {
        await openUrl(signup ? DEEZER_SIGNUP_URL : DEEZER_LOGIN_URL);
      } catch (err) {
        setState({ state: "error", message: deezerLoginErrorMessage(err) });
      }
    },
    [setState],
  );
}

/** Signs in when Deezer hands the ARL back as `deezer://autolog/<ARL>`,
 *  whether the app was already running or launched by the link. Mounted
 *  once, independently of the TIDAL session. */
export function useDeezerAutologListener() {
  const { login } = useDeezer();
  const setState = useSetAtom(deezerBrowserLoginAtom);
  const handled = useRef<string | null>(null);

  const handle = useCallback(
    (url: string | undefined) => {
      const arl = url ? parseDeezerAutologUrl(url) : null;
      if (!arl || handled.current === arl) return;
      handled.current = arl;
      setState({ state: "signing-in" });
      login(arl)
        .then(() => setState({ state: "idle" }))
        .catch((err) =>
          setState({ state: "error", message: deezerLoginErrorMessage(err) }),
        );
    },
    [login, setState],
  );

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    onOpenUrl((urls) => urls.forEach(handle))
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    getCurrent()
      .then((urls) => urls?.forEach(handle))
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [handle]);
}

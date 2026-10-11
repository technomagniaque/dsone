import { useCallback, useEffect, useRef } from "react";
import { useSetAtom, useStore } from "jotai";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrent, onOpenUrl } from "@tauri-apps/plugin-deep-link";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  deezerBrowserLoginAtom,
  deezerUserAtom,
  type DeezerBrowserLogin,
  type DeezerUser,
} from "../atoms/deezer";
import {
  DEEZER_SIGNUP_URL,
  deezerLoginErrorMessage,
  parseDeezerAutologUrl,
} from "../lib/deezerLogin";
import { loadDeezerFavoriteIds, useDeezer } from "./useDeezer";

/** Sign in: Deezer's own login page in an app window, whose `arl` cookie
 *  the backend picks up. Sign-up opens in the default browser instead and
 *  comes back through the deezer://autolog link. */
export function useOpenDeezerSignIn() {
  const setState = useSetAtom(deezerBrowserLoginAtom);
  return useCallback(
    async (signup = false) => {
      setState({ state: "waiting" });
      try {
        if (signup) await openUrl(DEEZER_SIGNUP_URL);
        else await invoke("start_deezer_login_window");
      } catch (err) {
        setState({ state: "error", message: deezerLoginErrorMessage(err) });
      }
    },
    [setState],
  );
}

/** Results of the sign-in window opened by `useOpenDeezerSignIn`. */
export function useDeezerLoginWindowEvents() {
  const store = useStore();
  useEffect(() => {
    const setState = (s: DeezerBrowserLogin) =>
      store.set(deezerBrowserLoginAtom, s);
    const pending = [
      listen<DeezerUser>("deezer-login-success", ({ payload }) => {
        store.set(deezerUserAtom, payload);
        loadDeezerFavoriteIds(store);
        setState({ state: "idle" });
      }),
      listen<string>("deezer-login-error", ({ payload }) =>
        setState({ state: "error", message: deezerLoginErrorMessage(payload) }),
      ),
      listen("deezer-login-cancelled", () => {
        if (store.get(deezerBrowserLoginAtom).state === "waiting")
          setState({ state: "idle" });
      }),
    ];
    return () => {
      pending.forEach((p) => p.then((un) => un()).catch(() => {}));
    };
  }, [store]);
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

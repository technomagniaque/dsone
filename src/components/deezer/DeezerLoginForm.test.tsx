import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { Provider, createStore } from "jotai";
import type { PropsWithChildren } from "react";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import DeezerLoginForm from "./DeezerLoginForm";
import { deezerFavoriteTrackIdsAtom, deezerUserAtom } from "../../atoms/deezer";

const USER = {
  id: 5,
  name: "Etienne",
  canStreamLossless: true,
  canStreamHq: true,
};

function renderForm(onSuccess = vi.fn()) {
  const store = createStore();
  const wrapper = ({ children }: PropsWithChildren) => (
    <Provider store={store}>{children}</Provider>
  );
  render(<DeezerLoginForm onSuccess={onSuccess} />, { wrapper });
  return { store, onSuccess };
}

const submit = () =>
  fireEvent.click(screen.getByRole("button", { name: "Sign in with Deezer" }));

describe("DeezerLoginForm", () => {
  afterEach(cleanup);
  beforeEach(() => vi.mocked(invoke).mockReset());

  it("explains where to find the ARL cookie", () => {
    renderForm();
    expect(screen.getByText(/Cookies/)).toBeTruthy();
    expect(screen.getByText("arl")).toBeTruthy();
  });

  it("keeps the submit button disabled until an ARL is entered", () => {
    renderForm();
    const button = screen.getByRole("button", {
      name: "Sign in with Deezer",
    }) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    fireEvent.change(screen.getByLabelText("ARL token"), {
      target: { value: "   " },
    });
    expect(button.disabled).toBe(true);
  });

  it("logs in with the trimmed ARL and stores the Deezer user", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "deezer_login") return USER;
      if (cmd === "deezer_get_favorite_tracks")
        return [{ id: 11, title: "a", duration: 1 }];
      return null;
    });
    const { store, onSuccess } = renderForm();
    fireEvent.change(screen.getByLabelText("ARL token"), {
      target: { value: "  abc123  " },
    });
    submit();

    await waitFor(() => expect(onSuccess).toHaveBeenCalledWith(USER));
    expect(invoke).toHaveBeenCalledWith("deezer_login", { arl: "abc123" });
    expect(store.get(deezerUserAtom)).toEqual(USER);
    await waitFor(() =>
      expect(store.get(deezerFavoriteTrackIdsAtom).has(11)).toBe(true),
    );
  });

  it("shows the backend's error and stays signed out when the ARL is rejected", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "deezer_login")
        throw JSON.stringify({ kind: "Api", message: "Invalid ARL" });
      return null;
    });
    const { store, onSuccess } = renderForm();
    fireEvent.change(screen.getByLabelText("ARL token"), {
      target: { value: "bad" },
    });
    submit();

    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toBe("Invalid ARL");
    expect(onSuccess).not.toHaveBeenCalled();
    expect(store.get(deezerUserAtom)).toBeNull();
  });
});

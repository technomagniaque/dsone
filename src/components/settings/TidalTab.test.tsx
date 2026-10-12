import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import type { PropsWithChildren } from "react";

const auth = { isAuthenticated: false, logout: vi.fn() };
const navigateTo = vi.fn();
vi.mock("../../hooks/useAuth", () => ({ useAuth: () => auth }));
vi.mock("../../hooks/useNavigation", () => ({
  useNavigation: () => ({ navigateTo }),
}));

import TidalTab from "./TidalTab";
import { userNameAtom } from "../../atoms/auth";

function renderTab() {
  const store = createStore();
  store.set(userNameAtom, "Etienne");
  const wrapper = ({ children }: PropsWithChildren) => (
    <Provider store={store}>{children}</Provider>
  );
  const onClose = vi.fn();
  render(<TidalTab onClose={onClose} />, { wrapper });
  return { onClose };
}

describe("TidalTab", () => {
  afterEach(() => {
    cleanup();
    navigateTo.mockReset();
    auth.logout.mockReset();
  });

  it("opens the TIDAL sign-in screen when signed out", () => {
    auth.isAuthenticated = false;
    const { onClose } = renderTab();
    fireEvent.click(screen.getByRole("button", { name: "Sign in to TIDAL" }));
    expect(onClose).toHaveBeenCalled();
    expect(navigateTo).toHaveBeenCalledWith({ type: "tidalLogin" });
  });

  it("shows the account and logs out when signed in", () => {
    auth.isAuthenticated = true;
    auth.logout.mockResolvedValue(undefined);
    renderTab();
    expect(screen.getByText("Etienne")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Log out" }));
    expect(auth.logout).toHaveBeenCalled();
  });
});

import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { setLang } from "../lib/i18n";
import { WindowControls, type WindowControlApi } from "./WindowControls";

function createWindowApi() {
  let resized: (() => void) | undefined;
  const api = {
    minimize: vi.fn(async () => undefined),
    toggleMaximize: vi.fn(async () => undefined),
    close: vi.fn(async () => undefined),
    isMaximized: vi.fn(async () => false),
    onResized: vi.fn(async (handler: () => void) => {
      resized = handler;
      return vi.fn();
    }),
  } satisfies WindowControlApi;
  return { api, getResized: () => resized };
}

describe("WindowControls", () => {
  beforeEach(() => setLang("en"));

  it("routes all Windows caption actions to the current Tauri window", async () => {
    const user = userEvent.setup();
    const { api } = createWindowApi();
    render(<WindowControls windowApi={api} />);

    await user.click(screen.getByRole("button", { name: "Minimize window" }));
    await user.click(screen.getByRole("button", { name: "Maximize window" }));
    await user.click(screen.getByRole("button", { name: "Close window" }));

    expect(api.minimize).toHaveBeenCalledOnce();
    expect(api.toggleMaximize).toHaveBeenCalledOnce();
    expect(api.close).toHaveBeenCalledOnce();
  });

  it("switches the maximize control to Restore after a resize", async () => {
    const { api, getResized } = createWindowApi();
    api.isMaximized.mockResolvedValueOnce(false).mockResolvedValueOnce(true);
    render(<WindowControls windowApi={api} />);

    await waitFor(() => expect(getResized()).toBeDefined());
    await act(async () => {
      getResized()?.();
    });

    expect(await screen.findByRole("button", { name: "Restore window" })).toBeInTheDocument();
  });
});

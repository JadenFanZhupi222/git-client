import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import {
  APP_SETTINGS_ENTRY_POINTS,
  settingsSectionForEntryPoint,
} from "./lib/settings";
import { ToastProvider } from "./components/Toast";
import appSource from "./App.tsx?raw";

const ipc = vi.hoisted(() => ({
  credentialStatus: vi.fn(async () => false),
  saveCredential: vi.fn(),
  testCredential: vi.fn(),
  clearCredential: vi.fn(),
  setUpstream: vi.fn(),
  fetchRemote: vi.fn(),
  pullRemote: vi.fn(),
  pushRemote: vi.fn(),
  undo: vi.fn(),
  redo: vi.fn(),
  checkoutBranch: vi.fn(),
  initRepo: vi.fn(),
}));

vi.mock("./ipc", () => ipc);
vi.mock("./lib/queries", () => ({
  useRepoWatch: vi.fn(),
  useCurrentBranch: () => ({ data: null }),
  useAheadBehind: () => ({ data: null }),
  useRemotes: () => ({ data: [] }),
  useRemoteList: () => ({ data: [] }),
  useUndoState: () => ({ data: null }),
  useBranches: () => ({ data: [] }),
  useRefs: () => ({ data: [] }),
  useSubmodules: () => ({ data: [] }),
  useWorktrees: () => ({ data: [] }),
  useSparseCheckout: () => ({ data: [] }),
  invalidateHistory: vi.fn(),
  invalidateWorktree: vi.fn(),
  qk: {},
}));

const EXPECTED_SECTIONS = {
  githubCommand: "github",
  githubPrPanel: "github",
  githubCreatePrDialog: "github",
  gitlabCommand: "gitlab",
  gitlabMrPanel: "gitlab",
  gitlabCreateMrDialog: "gitlab",
} as const;

describe("App settings integration", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.clearAllMocks();
    ipc.credentialStatus.mockResolvedValue(false);
  });

  it("routes every legacy credential entry point through the declarative wiring", () => {
    expect(
      Object.fromEntries(
        Object.entries(APP_SETTINGS_ENTRY_POINTS).map(([source, entryPoint]) => [
          source,
          settingsSectionForEntryPoint(entryPoint),
        ]),
      ),
    ).toEqual(EXPECTED_SECTIONS);
  });

  it("does not make either legacy token dialog reachable from App", () => {
    expect(appSource).not.toMatch(/Git(?:Hub|Lab)TokenDialog|(?:github|gitlab)TokenOpen/i);
  });

  it("restores focus to the persistent More trigger after opening Settings from its menu", async () => {
    const user = userEvent.setup();
    render(
      <QueryClientProvider client={new QueryClient()}>
        <ToastProvider>
          <App />
        </ToastProvider>
      </QueryClientProvider>,
    );

    const moreTrigger = screen.getByRole("button", { name: "More" });
    await user.click(moreTrigger);
    await user.click(screen.getByRole("button", { name: "Settings" }));
    const dialog = await screen.findByRole("dialog", { name: "Settings" });
    await waitFor(() => expect(dialog).not.toHaveAttribute("aria-busy", "true"));
    await user.click(screen.getByRole("button", { name: "Close" }));

    await waitFor(() => expect(moreTrigger).toHaveFocus());
  });
});

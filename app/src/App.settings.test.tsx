import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
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
vi.mock("./views/ChangesView", () => ({ ChangesView: () => <div /> }));
vi.mock("./views/HistoryView", () => ({ HistoryView: () => <div /> }));
vi.mock("./components/Sidebar", () => ({ Sidebar: () => <div /> }));
vi.mock("./components/BranchSwitcher", () => ({ BranchSwitcher: () => <div /> }));
vi.mock("./components/SyncBadge", () => ({ SyncBadge: () => <div /> }));
vi.mock("./components/StashMenu", () => ({ StashMenu: () => <div /> }));
vi.mock("./components/GithubPrPanel", () => ({
  GithubPrPanel: ({ onConfigureToken, onConfigureCredential }: { onConfigureToken: () => void; onConfigureCredential: (kind: "deepseek" | "github") => void }) => (
    <>
      <button onClick={onConfigureToken}>Configure token from GitHub PR panel</button>
      <button onClick={() => onConfigureCredential("deepseek")}>Configure DeepSeek from AI Review</button>
      <button onClick={() => onConfigureCredential("github")}>Configure GitHub from AI Review</button>
    </>
  ),
}));
vi.mock("./components/GithubCreatePrDialog", () => ({
  GithubCreatePrDialog: ({ onConfigureToken }: { onConfigureToken: () => void }) => (
    <button onClick={onConfigureToken}>Configure token from GitHub create dialog</button>
  ),
}));
vi.mock("./components/GitlabMrPanel", () => ({
  GitlabMrPanel: ({ onConfigureToken }: { onConfigureToken: () => void }) => (
    <button onClick={onConfigureToken}>Configure token from GitLab MR panel</button>
  ),
}));
vi.mock("./components/GitlabCreateMrDialog", () => ({
  GitlabCreateMrDialog: ({ onConfigureToken }: { onConfigureToken: () => void }) => (
    <button onClick={onConfigureToken}>Configure token from GitLab create dialog</button>
  ),
}));
vi.mock("./lib/queries", () => ({
  useRepoWatch: vi.fn(),
  useCurrentBranch: () => ({ data: "main" }),
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

type SettingsPath = {
  command: string;
  configureAction?: string;
  selectedTab: "DeepSeek" | "GitHub" | "GitLab";
};

const SETTINGS_PATHS: readonly SettingsPath[] = [
  {
    command: "Configure GitHub credential",
    selectedTab: "GitHub",
  },
  {
    command: "Configure GitLab credential",
    selectedTab: "GitLab",
  },
  {
    command: "查看当前分支 GitHub PR",
    configureAction: "Configure token from GitHub PR panel",
    selectedTab: "GitHub",
  },
  {
    command: "查看当前分支 GitHub PR",
    configureAction: "Configure DeepSeek from AI Review",
    selectedTab: "DeepSeek",
  },
  {
    command: "查看当前分支 GitHub PR",
    configureAction: "Configure GitHub from AI Review",
    selectedTab: "GitHub",
  },
  {
    command: "创建 GitHub PR",
    configureAction: "Configure token from GitHub create dialog",
    selectedTab: "GitHub",
  },
  {
    command: "查看当前分支 GitLab MR",
    configureAction: "Configure token from GitLab MR panel",
    selectedTab: "GitLab",
  },
  {
    command: "创建 GitLab MR",
    configureAction: "Configure token from GitLab create dialog",
    selectedTab: "GitLab",
  },
];

describe("App settings integration", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.clearAllMocks();
    Element.prototype.scrollIntoView = vi.fn();
    ipc.credentialStatus.mockResolvedValue(false);
  });

  it.each(SETTINGS_PATHS)(
    "opens the $selectedTab section through $command",
    async ({ command, configureAction, selectedTab }) => {
      localStorage.setItem("repo.last", "C:\\test-repo");
      const user = userEvent.setup();
      renderApp();
      await user.click(screen.getByTestId("resume-repo"));
      await screen.findByTestId("repo-shell");

      const palette = await openCommandPalette(user);
      await user.type(within(palette).getByPlaceholderText(/Type a command/), command);
      await user.click(within(palette).getByRole("button"));
      if (configureAction) {
        await user.click(await screen.findByRole("button", { name: configureAction }));
      }

      expect(await screen.findByRole("tab", { name: selectedTab })).toHaveAttribute(
        "aria-selected",
        "true",
      );
    },
  );

  it("does not make either legacy token dialog reachable from App", () => {
    expect(appSource).not.toMatch(/Git(?:Hub|Lab)TokenDialog|(?:github|gitlab)TokenOpen/i);
  });

  it("restores focus to the persistent More trigger after opening Settings from its menu", async () => {
    const user = userEvent.setup();
    renderApp();

    const moreTrigger = screen.getByRole("button", { name: "More" });
    await user.click(moreTrigger);
    await user.click(screen.getByRole("button", { name: "Settings" }));
    const close = await screen.findByRole("button", { name: "Close" });
    await waitFor(() => expect(close).toBeEnabled());
    await user.click(close);

    await waitFor(() => expect(moreTrigger).toHaveFocus());
  });
});

function renderApp() {
  return render(
    <QueryClientProvider client={new QueryClient()}>
      <ToastProvider>
        <App />
      </ToastProvider>
    </QueryClientProvider>,
  );
}

async function openCommandPalette(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByTestId("command-palette"));
  return screen.findByRole("dialog", { name: "Command palette" });
}

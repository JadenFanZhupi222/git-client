import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { ToastProvider } from "./components/Toast";
import { setLang } from "./lib/i18n";

const dialog = vi.hoisted(() => ({ open: vi.fn() }));
const ipc = vi.hoisted(() => ({
  setUpstream: vi.fn(),
  fetchRemote: vi.fn(),
  pullRemote: vi.fn(),
  pushRemote: vi.fn(),
  undo: vi.fn(),
  redo: vi.fn(),
  checkoutBranch: vi.fn(),
  discoverRepo: vi.fn(),
  initRepo: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-dialog", () => dialog);
vi.mock("./ipc", () => ipc);
vi.mock("./views/ChangesView", () => ({ ChangesView: () => <div /> }));
vi.mock("./views/HistoryView", () => ({ HistoryView: () => <div /> }));
vi.mock("./components/Sidebar", () => ({ Sidebar: () => <div /> }));
vi.mock("./components/BranchSwitcher", () => ({ BranchSwitcher: () => <div /> }));
vi.mock("./components/SyncBadge", () => ({ SyncBadge: () => <div /> }));
vi.mock("./components/StashMenu", () => ({ StashMenu: () => <div /> }));
vi.mock("./components/RemoteManager", () => ({
  RemoteManager: () => <div role="dialog" aria-label="Remote manager test" />,
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

describe("opening a repository", () => {
  beforeEach(() => {
    localStorage.clear();
    setLang("en");
    vi.clearAllMocks();
    ipc.initRepo.mockResolvedValue(undefined);
  });

  it("opens the discovered root when a repository subfolder is selected", async () => {
    dialog.open.mockResolvedValue("D:\\work\\project\\src");
    ipc.discoverRepo.mockResolvedValue("D:\\work\\project");
    const user = userEvent.setup();
    renderApp();

    await user.click(screen.getByTestId("pick-repo"));

    expect(ipc.discoverRepo).toHaveBeenCalledWith("D:\\work\\project\\src");
    expect(await screen.findByTestId("repo-shell")).toBeInTheDocument();
    await waitFor(() => expect(localStorage.getItem("repo.last")).toBe("D:\\work\\project"));
  });

  it("guides a plain folder through initialization and remote setup", async () => {
    dialog.open.mockResolvedValue("D:\\work\\plain-folder");
    ipc.discoverRepo.mockRejectedValue({ code: "REPO_NOT_FOUND", message: "not a repository" });
    const user = userEvent.setup();
    renderApp();

    await user.click(screen.getByTestId("pick-repo"));
    const guide = await screen.findByRole("dialog", { name: "This folder isn't a Git repository" });

    expect(screen.getByTestId("non-repository-path")).toHaveTextContent("D:\\work\\plain-folder");
    expect(document.getElementById("root")).toHaveAttribute("inert");
    expect(document.getElementById("root")).toHaveAttribute("aria-hidden", "true");
    expect(localStorage.getItem("repo.last")).toBeNull();

    await user.click(screen.getByRole("radio", { name: /Initialize, then add a remote/ }));
    await user.click(screen.getByRole("button", { name: "Initialize and configure remote" }));

    expect(ipc.initRepo).toHaveBeenCalledWith("D:\\work\\plain-folder");
    expect(guide).not.toBeInTheDocument();
    expect(document.getElementById("root")).not.toHaveAttribute("inert");
    expect(document.getElementById("root")).not.toHaveAttribute("aria-hidden");
    expect(await screen.findByRole("dialog", { name: "Remote manager test" })).toBeInTheDocument();
    await waitFor(() => expect(localStorage.getItem("repo.last")).toBe("D:\\work\\plain-folder"));
  });

  it("keeps the guide open and reports initialization errors inline", async () => {
    dialog.open.mockResolvedValue("D:\\work\\plain-folder");
    ipc.discoverRepo.mockRejectedValue({ code: "REPO_NOT_FOUND", message: "not a repository" });
    ipc.initRepo.mockRejectedValue({ code: "GIT_CLI_NOT_FOUND", message: "Git is unavailable" });
    const user = userEvent.setup();
    renderApp();

    await user.click(screen.getByTestId("pick-repo"));
    await user.click(await screen.findByRole("button", { name: "Initialize and open" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("Git is unavailable");
    expect(screen.getByRole("dialog", { name: "This folder isn't a Git repository" })).toBeInTheDocument();
    expect(localStorage.getItem("repo.last")).toBeNull();
  });
});

function renderApp() {
  const container = document.createElement("div");
  container.id = "root";
  document.body.appendChild(container);
  return render(
    <QueryClientProvider client={new QueryClient()}>
      <ToastProvider>
        <App />
      </ToastProvider>
    </QueryClientProvider>,
    { container },
  );
}

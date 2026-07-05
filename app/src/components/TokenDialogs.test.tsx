import { render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { setLang } from "../lib/i18n";
import { GitHubTokenDialog } from "./GitHubTokenDialog";
import { GitLabTokenDialog } from "./GitLabTokenDialog";
import { ToastProvider } from "./Toast";

const ipc = vi.hoisted(() => ({
  clearGithubToken: vi.fn().mockResolvedValue(undefined),
  hasGithubToken: vi.fn().mockResolvedValue(false),
  setGithubToken: vi.fn().mockResolvedValue(undefined),
  clearGitlabToken: vi.fn().mockResolvedValue(undefined),
  hasGitlabToken: vi.fn().mockResolvedValue(true),
  setGitlabToken: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("../ipc", () => ipc);

describe("collaboration token dialogs i18n", () => {
  afterEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    setLang("en");
  });

  it("renders GitHub token dialog copy in English", async () => {
    setLang("en");
    render(
      <ToastProvider>
        <GitHubTokenDialog onClose={vi.fn()} />
      </ToastProvider>,
    );

    expect(await screen.findByRole("dialog", { name: "GitHub token" })).toBeInTheDocument();
    expect(screen.getByText("Current status: Not set")).toBeInTheDocument();
    expect(screen.getByLabelText("Personal access token")).toHaveAttribute("placeholder", "github_pat_... or ghp_...");
    expect(screen.getByRole("button", { name: "Clear" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save" })).toBeInTheDocument();
  });

  it("renders GitLab token dialog copy in Chinese", async () => {
    setLang("zh");
    render(
      <ToastProvider>
        <GitLabTokenDialog onClose={vi.fn()} />
      </ToastProvider>,
    );

    expect(await screen.findByRole("dialog", { name: "GitLab token" })).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText("当前状态：已保存到系统凭据库")).toBeInTheDocument());
    expect(screen.getByLabelText("Personal access token")).toHaveAttribute("placeholder", "glpat-...");
    expect(screen.getByRole("button", { name: "清除" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "取消" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "保存" })).toBeInTheDocument();
  });
});

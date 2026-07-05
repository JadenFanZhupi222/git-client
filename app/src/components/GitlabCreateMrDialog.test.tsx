import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { setLang } from "../lib/i18n";
import { GitlabCreateMrDialog } from "./GitlabCreateMrDialog";
import { ToastProvider } from "./Toast";

const { openUrl } = vi.hoisted(() => ({
  openUrl: vi.fn<() => Promise<void>>().mockResolvedValue(undefined),
}));
const onClose = vi.fn();
const onConfigureToken = vi.fn();

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl,
}));

vi.mock("../ipc", () => ({
  hasGitlabToken: vi.fn().mockResolvedValue(true),
  getGitlabToken: vi.fn().mockResolvedValue("glpat_secret"),
}));

const remotes = [
  {
    name: "origin",
    url: "https://gitlab.com/team/project.git",
  },
];

describe("GitlabCreateMrDialog", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.clearAllMocks();
    localStorage.clear();
    setLang("en");
  });

  it("renders create MR copy in Chinese", () => {
    setLang("zh");
    render(
      <ToastProvider>
        <GitlabCreateMrDialog
          remotes={remotes}
          branch="feature/gitlab-create"
          preferredRemote="origin"
          onClose={onClose}
          onConfigureToken={onConfigureToken}
        />
      </ToastProvider>,
    );

    expect(screen.getByRole("dialog", { name: "创建 GitLab 合并请求" })).toBeInTheDocument();
    expect(screen.getByText("创建 GitLab MR")).toBeInTheDocument();
    expect(screen.getByLabelText("标题")).toBeInTheDocument();
    expect(screen.getByLabelText("源分支")).toBeInTheDocument();
    expect(screen.getByLabelText("目标分支")).toBeInTheDocument();
    expect(screen.getByLabelText("描述")).toBeInTheDocument();
    expect(screen.getByLabelText("草稿合并请求")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Token" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "取消" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "创建" })).toBeInTheDocument();
  });

  it("creates a GitLab merge request and opens it", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          iid: 7,
          title: "Ship GitLab create",
          web_url: "https://gitlab.com/team/project/-/merge_requests/7",
          draft: false,
          author: { username: "dev-a" },
          source_branch: "feature/gitlab-create",
          target_branch: "main",
        }),
        { status: 201 },
      ),
    );
    vi.stubGlobal("fetch", fetchMock);

    render(
      <ToastProvider>
        <GitlabCreateMrDialog
          remotes={remotes}
          branch="feature/gitlab-create"
          preferredRemote="origin"
          branches={[
            { name: "feature/gitlab-create", is_head: true },
            { name: "feature/other", is_head: false },
          ]}
          refs={[
            { name: "origin/main", kind: "remote" },
            { name: "origin/release", kind: "remote" },
          ]}
          onClose={onClose}
          onConfigureToken={onConfigureToken}
        />
      </ToastProvider>,
    );

    await userEvent.clear(screen.getByLabelText("Title"));
    await userEvent.type(screen.getByLabelText("Title"), "Ship GitLab create");
    expect(screen.getByLabelText("Source")).toHaveValue("feature/gitlab-create");
    expect(screen.getByLabelText("Target")).toHaveValue("main");
    await userEvent.selectOptions(screen.getByLabelText("Target"), "release");
    await userEvent.type(
      screen.getByLabelText("Description"),
      "Created from test",
    );
    await userEvent.click(screen.getByRole("button", { name: "Create" }));

    await waitFor(() => {
      expect(fetchMock).toHaveBeenCalledWith(
        "https://gitlab.com/api/v4/projects/team%2Fproject/merge_requests",
        expect.objectContaining({
          method: "POST",
          body: JSON.stringify({
            title: "Ship GitLab create",
            source_branch: "feature/gitlab-create",
            target_branch: "release",
            description: "Created from test",
            draft: false,
          }),
        }),
      );
    });
    expect(openUrl).toHaveBeenCalledWith(
      "https://gitlab.com/team/project/-/merge_requests/7",
    );
    expect(onClose).toHaveBeenCalled();
  });
});

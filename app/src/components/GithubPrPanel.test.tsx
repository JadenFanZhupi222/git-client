import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { GithubPrPanel } from "./GithubPrPanel";
import { ToastProvider } from "./Toast";

const { openUrl } = vi.hoisted(() => ({
  openUrl: vi.fn<() => Promise<void>>().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl,
}));

vi.mock("../ipc", () => ({
  hasGithubToken: vi.fn().mockResolvedValue(true),
  getGithubToken: vi.fn().mockResolvedValue("ghp_secret"),
}));

const remotes = [
  {
    name: "origin",
    url: "https://github.com/team/project.git",
  },
];

describe("GithubPrPanel", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.clearAllMocks();
  });

  it("refreshes pull request results", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify([
            {
              number: 1,
              title: "Old PR title",
              html_url: "https://github.com/team/project/pull/1",
              user: { login: "dev-a" },
              head: { ref: "feature/refresh", sha: "abc123" },
              base: { ref: "main" },
            },
          ]),
          { status: 200 },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify([
            {
              number: 1,
              title: "Updated PR title",
              html_url: "https://github.com/team/project/pull/1",
              user: { login: "dev-a" },
              head: { ref: "feature/refresh", sha: "abc123" },
              base: { ref: "main" },
            },
          ]),
          { status: 200 },
        ),
      );
    vi.stubGlobal("fetch", fetchMock);

    render(
      <ToastProvider>
        <GithubPrPanel
          remotes={remotes}
          branch="feature/refresh"
          preferredRemote="origin"
          onClose={vi.fn()}
          onConfigureToken={vi.fn()}
        />
      </ToastProvider>,
    );

    expect(await screen.findByText("#1 Old PR title")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Refresh" }));

    expect(await screen.findByText("#1 Updated PR title")).toBeInTheDocument();
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });
});

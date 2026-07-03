import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { GithubCreatePrDialog } from "./GithubCreatePrDialog";
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

describe("GithubCreatePrDialog", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.clearAllMocks();
  });

  it("uses selectable head and base branches when creating a pull request", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          number: 12,
          title: "Ship branch selectors",
          html_url: "https://github.com/team/project/pull/12",
          draft: false,
          user: { login: "dev-a" },
          head: { ref: "feature/selectors", sha: "abc123" },
          base: { ref: "develop" },
        }),
        { status: 201 },
      ),
    );
    vi.stubGlobal("fetch", fetchMock);

    render(
      <ToastProvider>
        <GithubCreatePrDialog
          remotes={remotes}
          branch="feature/selectors"
          preferredRemote="origin"
          branches={[
            { name: "feature/selectors", is_head: true },
            { name: "feature/other", is_head: false },
          ]}
          refs={[
            { name: "origin/main", kind: "remote" },
            { name: "origin/develop", kind: "remote" },
          ]}
          onClose={vi.fn()}
          onConfigureToken={vi.fn()}
        />
      </ToastProvider>,
    );

    expect(screen.getByLabelText("Head")).toHaveValue("feature/selectors");
    expect(screen.getByLabelText("Base")).toHaveValue("main");

    await userEvent.selectOptions(screen.getByLabelText("Base"), "develop");
    await userEvent.clear(screen.getByLabelText("Title"));
    await userEvent.type(screen.getByLabelText("Title"), "Ship branch selectors");
    await userEvent.click(screen.getByRole("button", { name: "Create" }));

    await waitFor(() => {
      expect(fetchMock).toHaveBeenCalledWith(
        "https://api.github.com/repos/team/project/pulls",
        expect.any(Object),
      );
    });
    expect(JSON.parse(fetchMock.mock.calls[0][1].body)).toEqual({
      title: "Ship branch selectors",
      head: "feature/selectors",
      base: "develop",
      body: "",
      draft: false,
    });
  });
});

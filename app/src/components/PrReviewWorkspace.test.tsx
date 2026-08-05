import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { setLang } from "../lib/i18n";
import { PrReviewWorkspace } from "./PrReviewWorkspace";

const ipc = vi.hoisted(() => ({
  getReviewPreflight: vi.fn(),
  startPrReview: vi.fn(),
  cancelPrReview: vi.fn(),
  submitPrReview: vi.fn(),
  onReviewProgress: vi.fn(),
}));
const opener = vi.hoisted(() => ({ openUrl: vi.fn() }));

vi.mock("../ipc", () => ipc);
vi.mock("@tauri-apps/plugin-opener", () => opener);

const target = { owner: "acme", repo: "rocket", pull_number: 17 };
const normalPreflight = {
  head_sha: "0123456789abcdef",
  total_patch_bytes: 300,
  requires_selection: false,
  files: [
    { path: "src/a.ts", patch_bytes: 100, reviewable: true },
    { path: "src/b.ts", patch_bytes: 200, reviewable: true },
    { path: "assets/logo.png", patch_bytes: 0, reviewable: false },
  ],
};
const finding = {
  id: "f1",
  severity: "high",
  path: "src/a.ts",
  side: "RIGHT",
  line: 12,
  title: "Null access",
  failure_scenario: "A missing value crashes the request.",
  explanation: "The value is optional but is dereferenced.",
  draft_comment: "Please guard this optional value.",
};
const result = {
  run_id: "server-run",
  head_sha: normalPreflight.head_sha,
  findings: [finding],
  usage: { input_tokens: 100, output_tokens: 25, tool_calls: 1 },
};

function renderWorkspace(overrides: Partial<React.ComponentProps<typeof PrReviewWorkspace>> = {}) {
  const props = {
    target,
    onClose: vi.fn(),
    onConfigureCredential: vi.fn(),
    ...overrides,
  };
  return { ...render(<PrReviewWorkspace {...props} />), props };
}

async function acceptAndStart(user: ReturnType<typeof userEvent.setup>) {
  const consent = screen.queryByRole("checkbox", { name: /I understand/ });
  if (consent) await user.click(consent);
  await user.click(screen.getByRole("button", { name: "Start review" }));
}

describe("PrReviewWorkspace", () => {
  beforeEach(() => {
    localStorage.clear();
    setLang("en");
    vi.clearAllMocks();
    ipc.getReviewPreflight.mockReset();
    ipc.onReviewProgress.mockReset();
    ipc.startPrReview.mockReset();
    ipc.cancelPrReview.mockReset();
    ipc.submitPrReview.mockReset();
    ipc.getReviewPreflight.mockResolvedValue(normalPreflight);
    ipc.onReviewProgress.mockResolvedValue(vi.fn());
    ipc.startPrReview.mockResolvedValue(result);
    ipc.cancelPrReview.mockResolvedValue(undefined);
    ipc.submitPrReview.mockResolvedValue({ review_id: 88, html_url: "https://github.com/acme/rocket/pull/17#pullrequestreview-88" });
  });

  it("loads preflight, auto-selects reviewable files, and disables non-reviewable files", async () => {
    renderWorkspace();
    expect(screen.getByText("Loading review preflight…")).toBeInTheDocument();
    expect(await screen.findByText("0123456")).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "src/a.ts" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "src/b.ts" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: /assets\/logo.png/ })).toBeDisabled();
    expect(screen.getByText("2 files · 300 bytes")).toBeInTheDocument();
  });

  it("requires explicit selection for large PRs and enforces file and byte preview limits", async () => {
    const files = Array.from({ length: 31 }, (_, index) => ({
      path: `src/${index}.ts`,
      patch_bytes: index === 0 ? 200_001 : 1,
      reviewable: true,
    }));
    ipc.getReviewPreflight.mockResolvedValue({
      head_sha: "large-sha",
      total_patch_bytes: 200_031,
      requires_selection: true,
      files,
    });
    const user = userEvent.setup();
    renderWorkspace();
    expect(await screen.findByText(/Select the files/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start review" })).toBeDisabled();
    await user.click(screen.getByRole("checkbox", { name: "src/0.ts" }));
    expect(screen.getByText(/200,001 bytes exceeds the 200,000 byte preview limit/)).toBeInTheDocument();
    await user.click(screen.getByRole("checkbox", { name: "src/0.ts" }));
    for (let index = 1; index < files.length; index += 1) {
      await user.click(screen.getByRole("checkbox", { name: `src/${index}.ts` }));
    }
    await user.click(screen.getByRole("checkbox", { name: "src/0.ts" }));
    expect(screen.getByText(/30 file preview limit/)).toBeInTheDocument();
  });

  it("does not start before consent and persists the versioned consent", async () => {
    const user = userEvent.setup();
    renderWorkspace();
    await screen.findByText("0123456");
    expect(screen.getByRole("button", { name: "Start review" })).toBeDisabled();
    expect(ipc.startPrReview).not.toHaveBeenCalled();
    await user.click(screen.getByRole("checkbox", { name: /I understand/ }));
    expect(localStorage.getItem("pr-review-consent-v1")).toBe("accepted");
    await user.click(screen.getByRole("button", { name: "Start review" }));
    await screen.findByText("Review findings");
    expect(ipc.startPrReview).toHaveBeenCalledTimes(1);
  });

  it("subscribes before invoking, filters run ids, shows tool progress, and unsubscribes", async () => {
    const calls: string[] = [];
    let emit: ((event: { run_id: string; stage: string; tool_name: string | null; tool_calls: number | null }) => void) | undefined;
    const unsubscribe = vi.fn();
    ipc.onReviewProgress.mockImplementation(async (callback) => {
      calls.push("subscribe"); emit = callback; return unsubscribe;
    });
    let finish!: (value: typeof result) => void;
    ipc.startPrReview.mockImplementation(() => {
      calls.push("start");
      return new Promise((resolve) => { finish = resolve; });
    });
    const user = userEvent.setup();
    renderWorkspace();
    await screen.findByText("0123456");
    await acceptAndStart(user);
    await waitFor(() => expect(calls).toEqual(["subscribe", "start"]));
    const runId = ipc.startPrReview.mock.calls[0][0].run_id;
    emit?.({ run_id: "other", stage: "failed", tool_name: null, tool_calls: null });
    emit?.({ run_id: runId, stage: "tool_call", tool_name: "get_file_excerpt", tool_calls: 2 });
    expect(await screen.findByText("Calling tool: get_file_excerpt · 2 calls")).toBeInTheDocument();
    finish({ ...result, run_id: runId });
    await screen.findByText("Review findings");
    expect(unsubscribe).toHaveBeenCalledTimes(1);
  });

  it("cancels a running review and blocks close until terminal", async () => {
    let finish!: (value: typeof result) => void;
    ipc.startPrReview.mockReturnValue(new Promise((resolve) => { finish = resolve; }));
    const user = userEvent.setup();
    const { props } = renderWorkspace();
    await screen.findByText("0123456");
    await acceptAndStart(user);
    fireEvent.click(screen.getByTestId("pr-review-backdrop"));
    fireEvent.keyDown(window, { key: "Escape" });
    await user.click(screen.getByRole("button", { name: "Close AI review" }));
    expect(props.onClose).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Cancel review" }));
    expect(screen.getByRole("button", { name: "Cancelling…" })).toBeDisabled();
    expect(ipc.cancelPrReview).toHaveBeenCalledWith(ipc.startPrReview.mock.calls[0][0].run_id);
    finish({ ...result, findings: [] });
  });

  it("traps initial focus and restores focus on close", async () => {
    const trigger = document.createElement("button");
    document.body.appendChild(trigger);
    trigger.focus();
    const view = renderWorkspace();
    const dialog = screen.getByRole("dialog", { name: "AI Review" });
    await waitFor(() => expect(dialog).toHaveFocus());
    view.unmount();
    expect(trigger).toHaveFocus();
    trigger.remove();
  });

  it.each([
    ["AI_KEY_MISSING", "deepseek"],
    ["GITHUB_TOKEN_MISSING", "github"],
  ] as const)("routes %s to credential settings", async (code, kind) => {
    ipc.startPrReview.mockRejectedValue({ code, message: "Missing credential", recoverable: true });
    const user = userEvent.setup();
    const { props } = renderWorkspace();
    await screen.findByText("0123456");
    await acceptAndStart(user);
    await user.click(await screen.findByRole("button", { name: "Open settings" }));
    expect(props.onConfigureCredential).toHaveBeenCalledWith(kind);
  });

  it("requires a new preflight after PR_UPDATED", async () => {
    ipc.startPrReview.mockRejectedValue({ code: "PR_UPDATED", message: "Updated", recoverable: true });
    const user = userEvent.setup();
    renderWorkspace();
    await screen.findByText("0123456");
    await acceptAndStart(user);
    await user.click(await screen.findByRole("button", { name: "Refresh preflight" }));
    expect(ipc.getReviewPreflight).toHaveBeenCalledTimes(2);
  });

  it("edits and selects findings and submits one batch with the pinned head", async () => {
    localStorage.setItem("pr-review-consent-v1", "accepted");
    const user = userEvent.setup();
    renderWorkspace();
    await screen.findByText("0123456");
    await acceptAndStart(user);
    expect(await screen.findByText("Reviewed files: src/a.ts, src/b.ts")).toBeInTheDocument();
    const card = await screen.findByRole("group", { name: "High: Null access" });
    const editor = within(card).getByRole("textbox", { name: "Draft comment" });
    await user.clear(editor);
    await user.type(editor, "Edited comment");
    await user.click(screen.getByRole("button", { name: "Submit review" }));
    expect(ipc.submitPrReview).toHaveBeenCalledWith({
      target,
      head_sha: normalPreflight.head_sha,
      findings: [{ ...finding, draft_comment: "Edited comment" }],
    });
    expect(await screen.findByText("Review published")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Open on GitHub" }));
    expect(opener.openUrl).toHaveBeenCalledWith("https://github.com/acme/rocket/pull/17#pullrequestreview-88");
  });

  it("locks finding edits while the submitted batch is pending", async () => {
    localStorage.setItem("pr-review-consent-v1", "accepted");
    ipc.submitPrReview.mockReturnValue(new Promise(() => undefined));
    const user = userEvent.setup();
    renderWorkspace();
    await screen.findByText("0123456");
    await acceptAndStart(user);
    const editor = await screen.findByRole("textbox", { name: "Draft comment" });
    const include = screen.getByRole("checkbox", { name: "Include finding: Null access" });

    await user.click(screen.getByRole("button", { name: "Submit review" }));

    expect(editor).toBeDisabled();
    expect(include).toBeDisabled();
    expect(screen.getByRole("button", { name: "Close AI review" })).toBeDisabled();
  });

  it("treats no findings as a valid result and disables submission", async () => {
    localStorage.setItem("pr-review-consent-v1", "accepted");
    ipc.startPrReview.mockResolvedValue({ ...result, findings: [] });
    const user = userEvent.setup();
    renderWorkspace();
    await screen.findByText("0123456");
    await acceptAndStart(user);
    expect(await screen.findByText("No actionable issues found.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Submit review" })).toBeDisabled();
  });

  it("preserves edited drafts after a publish failure and retries the same batch", async () => {
    localStorage.setItem("pr-review-consent-v1", "accepted");
    ipc.submitPrReview
      .mockRejectedValueOnce({ code: "REVIEW_PUBLISH_FAILED", message: "failed", recoverable: true })
      .mockResolvedValueOnce({ review_id: 89, html_url: null });
    const user = userEvent.setup();
    renderWorkspace();
    await screen.findByText("0123456");
    await acceptAndStart(user);
    const editor = await screen.findByRole("textbox", { name: "Draft comment" });
    await user.clear(editor);
    await user.type(editor, "Keep this edit");
    await user.click(screen.getByRole("button", { name: "Submit review" }));
    expect(await screen.findByText(/could not publish/)).toBeInTheDocument();
    expect(editor).toHaveValue("Keep this edit");
    await user.click(screen.getByRole("button", { name: "Submit review" }));
    expect(await screen.findByText("Review published")).toBeInTheDocument();
    expect(ipc.submitPrReview.mock.calls[1][0].findings[0].draft_comment).toBe("Keep this edit");
  });

  it("requires re-review when the PR changes during submission", async () => {
    localStorage.setItem("pr-review-consent-v1", "accepted");
    ipc.submitPrReview.mockRejectedValue({ code: "PR_UPDATED", message: "updated", recoverable: true });
    const user = userEvent.setup();
    renderWorkspace();
    await screen.findByText("0123456");
    await acceptAndStart(user);
    await user.click(await screen.findByRole("button", { name: "Submit review" }));
    await user.click(await screen.findByRole("button", { name: "Refresh preflight" }));
    expect(ipc.getReviewPreflight).toHaveBeenCalledTimes(2);
  });

  it("does not update after unmount while listener setup is pending", async () => {
    localStorage.setItem("pr-review-consent-v1", "accepted");
    let resolveListener!: (unsubscribe: () => void) => void;
    ipc.onReviewProgress.mockReturnValue(new Promise((resolve) => { resolveListener = resolve; }));
    const user = userEvent.setup();
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const view = renderWorkspace();
    await screen.findByText("0123456");
    await user.click(screen.getByRole("button", { name: "Start review" }));
    view.unmount();
    const unsubscribe = vi.fn();
    resolveListener(unsubscribe);
    await waitFor(() => expect(unsubscribe).toHaveBeenCalled());
    expect(consoleError).not.toHaveBeenCalled();
    consoleError.mockRestore();
  });

  it("cancels an active review when the workspace unmounts", async () => {
    localStorage.setItem("pr-review-consent-v1", "accepted");
    ipc.startPrReview.mockReturnValue(new Promise(() => undefined));
    const user = userEvent.setup();
    const view = renderWorkspace();
    await screen.findByText("0123456");
    await user.click(screen.getByRole("button", { name: "Start review" }));
    const runId = ipc.startPrReview.mock.calls[0][0].run_id;

    view.unmount();

    expect(ipc.cancelPrReview).toHaveBeenCalledWith(runId);
  });
});

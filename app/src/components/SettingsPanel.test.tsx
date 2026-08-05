import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { setLang } from "../lib/i18n";
import { ToastProvider } from "./Toast";
import { SettingsPanel } from "./SettingsPanel";

const ipc = vi.hoisted(() => ({
  credentialStatus: vi.fn(),
  saveCredential: vi.fn(),
  testCredential: vi.fn(),
  clearCredential: vi.fn(),
}));

vi.mock("../ipc", () => ipc);

function renderPanel(
  props: Partial<React.ComponentProps<typeof SettingsPanel>> = {},
) {
  const onClose = vi.fn();
  render(
    <ToastProvider>
      <SettingsPanel onClose={onClose} {...props} />
    </ToastProvider>,
  );
  return { onClose };
}

describe("SettingsPanel", () => {
  beforeEach(() => {
    setLang("en");
    ipc.credentialStatus.mockImplementation(async (kind: string) => kind === "github");
    ipc.saveCredential.mockResolvedValue(undefined);
    ipc.testCredential.mockResolvedValue(undefined);
    ipc.clearCredential.mockResolvedValue(undefined);
  });

  it("shows one Integrations settings category without provider names in category navigation", async () => {
    renderPanel();

    const categories = screen.getByRole("navigation", { name: "Settings categories" });
    const integration = within(categories).getByText("Integrations");
    expect(integration).toHaveAttribute("aria-current", "page");
    expect(within(categories).queryByText("DeepSeek")).not.toBeInTheDocument();
    expect(within(categories).queryByText("GitHub")).not.toBeInTheDocument();
    expect(within(categories).queryByText("GitLab")).not.toBeInTheDocument();
    expect(screen.getByRole("tablist", { name: "Integration providers" })).toBeInTheDocument();
  });

  it("selects the requested provider within the Integrations category", async () => {
    renderPanel({ initialSection: "github" });

    const providers = screen.getByRole("tablist", { name: "Integration providers" });
    expect(within(providers).getByRole("tab", { name: "GitHub" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(document.getElementById("settings-panel-github")).not.toHaveAttribute("hidden");
  });

  it("shows only a right-aligned Save credential action for an unconfigured provider", async () => {
    const user = userEvent.setup();
    renderPanel({ initialSection: "deepseek" });
    await screen.findByText("Not configured");

    const save = screen.getByRole("button", { name: "Save credential" });
    expect(save).toBeDisabled();
    expect(save).toHaveClass("ml-auto");
    expect(screen.queryByRole("button", { name: "Remove credential" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Test connection" })).not.toBeInTheDocument();

    await user.type(screen.getByLabelText("DeepSeek API key"), "new-key");
    expect(save).toBeEnabled();
  });

  it("shows replacement actions and copy without exposing the configured credential", async () => {
    const user = userEvent.setup();
    renderPanel({ initialSection: "github" });
    await screen.findByText("Configured");
    expect(screen.queryByText("Connected")).not.toBeInTheDocument();

    const input = screen.getByLabelText("GitHub personal access token");
    expect(input).toHaveValue("");
    expect(input).toHaveAttribute("placeholder", "Enter a new credential to replace the saved one");
    expect(screen.getByText("Stored securely in your system credential store.")).toBeInTheDocument();
    const save = screen.getByRole("button", { name: "Save replacement" });
    const test = screen.getByRole("button", { name: "Test connection" });
    const remove = screen.getByRole("button", { name: "Remove credential" });
    const actions = save.parentElement!;
    expect(within(actions).getAllByRole("button")).toEqual([save, test, remove]);
    expect(actions).toHaveClass("flex-col", "min-[441px]:flex-row");
    expect(remove).toHaveClass("min-[441px]:order-1");
    expect(test).toHaveClass("min-[441px]:order-2", "min-[441px]:ml-auto");
    expect(save).toHaveClass("min-[441px]:order-3");
    expect(save).toBeDisabled();

    await user.type(input, "replacement");
    expect(save).toBeEnabled();
    expect(document.body).not.toHaveTextContent("stored-secret");
  });

  it("renders flat DeepSeek-only service details, sentence-case label, and credential helper", async () => {
    const user = userEvent.setup();
    renderPanel({ initialSection: "deepseek" });
    await screen.findByText("Not configured");

    const heading = screen.getByRole("heading", { name: "Service details" });
    const details = heading.parentElement!;
    expect(details.querySelector("dl")).toBeInTheDocument();
    expect(within(details).getByText("Endpoint").tagName).toBe("DT");
    expect(within(details).getByText("Model").tagName).toBe("DT");
    expect(within(details).getByText("https://api.deepseek.com").tagName).toBe("DD");
    expect(within(details).getByText(/PR patches and only the code excerpts/)).toHaveClass("text-fg-muted");
    expect(details).not.toHaveClass("rounded-md", "border", "bg-elevated");
    const label = screen.getByText("DeepSeek API key");
    expect(label).toHaveClass("text-xs");
    expect(label).not.toHaveClass("uppercase", "tracking-wide");
    expect(screen.getByLabelText("DeepSeek API key")).toHaveClass("h-9", "field");
    expect(screen.getByText("Stored securely in your system credential store.")).toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: "GitHub" }));
    expect(screen.queryByRole("heading", { name: "Service details" })).not.toBeInTheDocument();
  });

  it("renders Chinese credential labels and configured actions", async () => {
    setLang("zh");
    renderPanel({ initialSection: "github" });
    await screen.findByText("已配置");

    expect(screen.getByLabelText("GitHub 个人访问令牌")).toHaveValue("");
    expect(screen.getByPlaceholderText("输入新凭据将替换已保存的凭据")).toBeInTheDocument();
    expect(screen.getByText("凭据将安全存储在系统凭据存储中。")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "保存替换凭据" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "测试连接" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "移除凭据" })).toBeInTheDocument();
  });

  it("uses one constrained vertical scroll owner for provider content", () => {
    renderPanel();

    const dialog = screen.getByRole("dialog", { name: "Settings" });
    const activePanel = screen.getByRole("tabpanel", { name: "DeepSeek" });
    const scrollOwner = activePanel.parentElement;
    expect(scrollOwner).toHaveClass("min-h-0", "flex-1", "overflow-y-auto");
    expect(activePanel).not.toHaveClass("overflow-y-auto");
    expect(dialog.querySelectorAll(".overflow-y-auto")).toHaveLength(1);
  });

  it("stacks categories above full-width content and keeps provider tabs scrollable on narrow screens", () => {
    renderPanel();

    const categories = screen.getByRole("navigation", { name: "Settings categories" });
    const layout = categories.parentElement;
    const providers = screen.getByRole("tablist", { name: "Integration providers" });
    expect(layout).toHaveClass(
      "grid-cols-1",
      "grid-rows-[auto_minmax(0,1fr)]",
      "sm:grid-cols-[150px_minmax(0,1fr)]",
      "sm:grid-rows-1",
    );
    expect(categories).toHaveClass("border-b", "sm:border-b-0", "sm:border-r");
    expect(providers).toHaveClass("overflow-x-auto");
    for (const tab of within(providers).getAllByRole("tab")) {
      expect(tab).toHaveClass("shrink-0");
    }
  });

  afterEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    setLang("en");
  });

  it("renders every provider and the fixed DeepSeek service details", async () => {
    renderPanel();

    expect(screen.getByRole("dialog", { name: "Settings" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "DeepSeek" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "GitHub" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "GitLab" })).toBeInTheDocument();
    const tabs = screen.getAllByRole("tab");
    const panels = screen.getAllByRole("tabpanel", { hidden: true });
    expect(panels).toHaveLength(3);
    for (const tab of tabs) {
      const panelId = tab.getAttribute("aria-controls");
      expect(panelId).toBeTruthy();
      expect(document.getElementById(panelId!)).toHaveAttribute("role", "tabpanel");
      expect(document.getElementById(panelId!)).toHaveAttribute("aria-labelledby", tab.id);
    }
    expect(document.getElementById("settings-panel-deepseek")).not.toHaveAttribute("hidden");
    expect(document.getElementById("settings-panel-github")).toHaveAttribute("hidden");
    expect(document.getElementById("settings-panel-gitlab")).toHaveAttribute("hidden");
    expect(screen.getByRole("tab", { name: "DeepSeek" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tab", { name: "GitHub" })).toHaveAttribute("aria-selected", "false");
    expect(screen.getByRole("tab", { name: "GitLab" })).toHaveAttribute("aria-selected", "false");
    expect(screen.getByText("https://api.deepseek.com")).toBeInTheDocument();
    expect(screen.getByText("deepseek-v4-flash")).toBeInTheDocument();
    expect(screen.getByText(/PR patches and only the code excerpts you request/)).toBeInTheDocument();

    await waitFor(() => expect(ipc.credentialStatus).toHaveBeenCalledTimes(3));
    expect(ipc.credentialStatus).toHaveBeenCalledWith("deepseek");
    expect(ipc.credentialStatus).toHaveBeenCalledWith("github");
    expect(ipc.credentialStatus).toHaveBeenCalledWith("gitlab");
  });

  it("opens the requested initial section and focuses its empty password input", async () => {
    renderPanel({ initialSection: "gitlab" });

    const tab = screen.getByRole("tab", { name: "GitLab" });
    expect(tab).toHaveAttribute("aria-selected", "true");
    const input = screen.getByLabelText("GitLab personal access token");
    expect(input).toHaveValue("");
    await waitFor(() => expect(input).toHaveFocus());
  });

  it("shows configured status without ever rendering a saved secret", async () => {
    renderPanel({ initialSection: "github" });

    expect(await screen.findByText("Configured")).toBeInTheDocument();
    const input = screen.getByLabelText("GitHub personal access token");
    expect(input).toHaveValue("");
    expect(document.body).not.toHaveTextContent("stored-secret");
  });

  it("passes the unmodified secret to backend save, then clears it from the DOM", async () => {
    const user = userEvent.setup();
    renderPanel({ initialSection: "github" });
    await screen.findByText("Configured");
    const input = screen.getByLabelText("GitHub personal access token");

    await user.type(input, "  private-secret  ");
    await user.click(screen.getByRole("button", { name: "Save replacement" }));

    await waitFor(() =>
      expect(ipc.saveCredential).toHaveBeenCalledWith("github", "  private-secret  "),
    );
    expect(input).toHaveValue("");
    expect(document.body).not.toHaveTextContent("private-secret");
    expect(await screen.findByText("GitHub credential saved")).toBeInTheDocument();
  });

  it("tests and clears the saved credential while updating status", async () => {
    const user = userEvent.setup();
    renderPanel({ initialSection: "github" });
    await screen.findByText("Configured");

    await user.click(screen.getByRole("button", { name: "Test connection" }));
    await waitFor(() => expect(ipc.testCredential).toHaveBeenCalledWith("github"));
    expect(await screen.findByText("GitHub credential is valid")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Remove credential" }));
    await waitFor(() => expect(ipc.clearCredential).toHaveBeenCalledWith("github"));
    expect(await screen.findByText("Not configured")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Test connection" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Remove credential" })).not.toBeInTheDocument();
  });

  it("shows the IpcError message in an error toast", async () => {
    const user = userEvent.setup();
    ipc.saveCredential.mockRejectedValue({ code: "SAVE_FAILED", message: "Credential store unavailable" });
    renderPanel({ initialSection: "deepseek" });
    await screen.findByText("Not configured");

    const input = screen.getByLabelText("DeepSeek API key");
    await user.type(input, "private-secret");
    await user.click(screen.getByRole("button", { name: "Save credential" }));

    expect(await screen.findByText("Credential store unavailable")).toBeInTheDocument();
    expect(input).toHaveValue("private-secret");
  });

  it("keeps successful provider statuses when another status lookup fails", async () => {
    ipc.credentialStatus.mockImplementation(async (kind: string) => {
      if (kind === "deepseek") throw { code: "STATUS_FAILED", message: "DeepSeek status unavailable" };
      return kind === "github";
    });
    renderPanel({ initialSection: "github" });

    expect(await screen.findByText("Configured")).toBeInTheDocument();
    expect(await screen.findByText("DeepSeek status unavailable")).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole("tab", { name: "DeepSeek" }));
    expect(screen.getByText("Status unavailable")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save credential" })).toBeDisabled();
    await userEvent.setup().type(screen.getByLabelText("DeepSeek API key"), "recovery-key");
    expect(screen.getByRole("button", { name: "Save credential" })).toBeEnabled();
  });

  it("supports tablist navigation, contains focus, and restores focus on close", async () => {
    const user = userEvent.setup();
    const trigger = document.createElement("button");
    trigger.textContent = "Open settings";
    document.body.appendChild(trigger);
    trigger.focus();
    const { unmount } = render(
      <ToastProvider>
        <SettingsPanel onClose={vi.fn()} initialSection="github" />
      </ToastProvider>,
    );
    await screen.findByText("Configured");

    const githubTab = screen.getByRole("tab", { name: "GitHub" });
    githubTab.focus();
    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: "GitLab" })).toHaveFocus();
    expect(screen.getByRole("tab", { name: "GitLab" })).toHaveAttribute("aria-selected", "true");
    expect(document.getElementById("settings-panel-github")).toHaveAttribute("hidden");
    expect(document.getElementById("settings-panel-gitlab")).not.toHaveAttribute("hidden");

    await user.keyboard("{Home}");
    expect(screen.getByRole("tab", { name: "DeepSeek" })).toHaveFocus();
    expect(screen.getByRole("tab", { name: "DeepSeek" })).toHaveAttribute("aria-selected", "true");
    await user.keyboard("{ArrowLeft}");
    expect(screen.getByRole("tab", { name: "GitLab" })).toHaveFocus();
    await user.keyboard("{End}");
    expect(screen.getByRole("tab", { name: "GitLab" })).toHaveFocus();

    await user.type(screen.getByLabelText("GitLab personal access token"), "secret");
    const close = within(screen.getByRole("dialog", { name: "Settings" })).getByRole("button", { name: "Close" });
    close.focus();
    await user.keyboard("{Shift>}{Tab}{/Shift}");
    expect(screen.getByRole("button", { name: "Save credential" })).toHaveFocus();
    await user.keyboard("{Tab}");
    expect(close).toHaveFocus();

    unmount();
    expect(trigger).toHaveFocus();
    trigger.remove();
  });

  it("restores focus to a stable supplied target when the opener disconnects", async () => {
    const stableTarget = document.createElement("button");
    const transientOpener = document.createElement("button");
    document.body.append(stableTarget, transientOpener);
    transientOpener.focus();
    const returnFocusRef = { current: stableTarget };
    const { unmount } = render(
      <ToastProvider>
        <SettingsPanel
          onClose={vi.fn()}
          initialSection="github"
          returnFocusRef={returnFocusRef}
        />
      </ToastProvider>,
    );
    transientOpener.remove();

    unmount();

    expect(stableTarget).toHaveFocus();
    stableTarget.remove();
  });

  it("keeps and restores focus while statuses load", async () => {
    const trigger = document.createElement("button");
    trigger.textContent = "Open settings";
    document.body.appendChild(trigger);
    trigger.focus();
    let resolveStatuses!: (value: boolean) => void;
    const statusPromise = new Promise<boolean>((resolve) => { resolveStatuses = resolve; });
    ipc.credentialStatus.mockImplementation(() => statusPromise);
    const user = userEvent.setup();
    const { unmount } = render(
      <ToastProvider>
        <SettingsPanel onClose={vi.fn()} initialSection="github" />
      </ToastProvider>,
    );
    const dialog = screen.getByRole("dialog", { name: "Settings" });

    await waitFor(() => expect(dialog).toHaveFocus());
    trigger.focus();
    await user.keyboard("{Tab}");
    expect(dialog).toHaveFocus();

    unmount();
    expect(trigger).toHaveFocus();
    trigger.remove();
    resolveStatuses(true);
  });

  it("keeps and restores focus while an operation is busy", async () => {
    const trigger = document.createElement("button");
    trigger.textContent = "Open settings";
    document.body.appendChild(trigger);
    trigger.focus();
    const user = userEvent.setup();
    const { unmount } = render(
      <ToastProvider>
        <SettingsPanel onClose={vi.fn()} initialSection="github" />
      </ToastProvider>,
    );
    await screen.findByText("Configured");
    let resolveTest!: () => void;
    ipc.testCredential.mockImplementation(() => new Promise<void>((resolve) => { resolveTest = resolve; }));
    await user.click(screen.getByRole("button", { name: "Test connection" }));
    const dialog = screen.getByRole("dialog", { name: "Settings" });
    expect(dialog).toHaveFocus();
    trigger.focus();
    await user.keyboard("{Tab}");
    expect(dialog).toHaveFocus();

    unmount();
    expect(trigger).toHaveFocus();
    trigger.remove();
    resolveTest();
  });

  it("closes with Escape or backdrop only while no operation is active", async () => {
    const user = userEvent.setup();
    const { onClose } = renderPanel();
    await screen.findByText("Not configured");

    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(1);
    await user.click(screen.getByTestId("settings-backdrop"));
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it("disables conflicting controls and blocks closing while saving", async () => {
    let resolveSave!: () => void;
    ipc.saveCredential.mockImplementation(() => new Promise<void>((resolve) => { resolveSave = resolve; }));
    const user = userEvent.setup();
    const { onClose } = renderPanel({ initialSection: "github" });
    await screen.findByText("Configured");

    await user.type(screen.getByLabelText("GitHub personal access token"), "private-secret");
    await user.click(screen.getByRole("button", { name: "Save replacement" }));

    const dialog = screen.getByRole("dialog", { name: "Settings" });
    expect(within(dialog).getByRole("button", { name: "Save replacement" })).toBeDisabled();
    expect(within(dialog).getByRole("button", { name: "Test connection" })).toBeDisabled();
    expect(within(dialog).getByRole("button", { name: "Remove credential" })).toBeDisabled();
    expect(within(dialog).getByRole("button", { name: "Close" })).toBeDisabled();
    await user.keyboard("{Escape}");
    await user.click(screen.getByTestId("settings-backdrop"));
    expect(onClose).not.toHaveBeenCalled();

    resolveSave();
    await waitFor(() =>
      expect(within(dialog).getByRole("button", { name: "Close" })).toBeEnabled(),
    );
  });

  it.each(["save", "test", "clear"] as const)(
    "ignores a pending %s completion after unmount",
    async (operation) => {
      let resolveOperation!: () => void;
      ipc[`${operation}Credential`].mockImplementation(
        () => new Promise<void>((resolve) => { resolveOperation = resolve; }),
      );
      const user = userEvent.setup();
      const stableTarget = document.createElement("button");
      document.body.appendChild(stableTarget);
      stableTarget.focus();
      const view = render(
        <ToastProvider>
          <SettingsPanel onClose={vi.fn()} initialSection="github" />
        </ToastProvider>,
      );
      await screen.findByText("Configured");
      if (operation === "save") {
        await user.type(screen.getByLabelText("GitHub personal access token"), "secret");
      }
      await user.click(screen.getByRole("button", { name: operation === "clear" ? "Remove credential" : operation === "test" ? "Test connection" : "Save replacement" }));
      view.rerender(<ToastProvider><div /></ToastProvider>);
      stableTarget.focus();

      resolveOperation();
      await Promise.resolve();
      await Promise.resolve();

      expect(screen.queryByText(/GitHub credential (saved|is valid|cleared)/)).not.toBeInTheDocument();
      expect(stableTarget).toHaveFocus();
      stableTarget.remove();
    },
  );

  it("does not toast when status loading rejects after unmount", async () => {
    let rejectStatus!: (reason: unknown) => void;
    ipc.credentialStatus.mockImplementation(
      () => new Promise<boolean>((_resolve, reject) => { rejectStatus = reject; }),
    );
    const view = render(
      <ToastProvider>
        <SettingsPanel onClose={vi.fn()} />
      </ToastProvider>,
    );
    view.rerender(<ToastProvider><div /></ToastProvider>);

    rejectStatus({ code: "STATUS_FAILED", message: "late failure" });
    await Promise.resolve();
    await Promise.resolve();

    expect(screen.queryByText("late failure")).not.toBeInTheDocument();
  });
});

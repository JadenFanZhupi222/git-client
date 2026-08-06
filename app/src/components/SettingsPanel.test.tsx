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
const opener = vi.hoisted(() => ({ openUrl: vi.fn().mockResolvedValue(undefined) }));

vi.mock("../ipc", () => ipc);
vi.mock("@tauri-apps/plugin-opener", () => opener);

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
    expect(screen.getByRole("tablist", { name: "Integration" })).toBeInTheDocument();
    expect(screen.getByText("Manage credentials for AI review and code hosting.")).toBeInTheDocument();
  });

  it("selects the requested provider within the Integrations category", async () => {
    renderPanel({ initialSection: "github" });

    const providers = screen.getByRole("tablist", { name: "Integration" });
    expect(within(providers).getByRole("tab", { name: "GitHub" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(document.getElementById("settings-panel-github")).not.toHaveAttribute("hidden");
  });

  it("explains hosting token prefixes outside the input placeholders", async () => {
    const user = userEvent.setup();
    ipc.credentialStatus.mockResolvedValue(false);
    const english = render(
      <ToastProvider>
        <SettingsPanel onClose={vi.fn()} initialSection="github" />
      </ToastProvider>,
    );
    await screen.findByText("Not configured");
    expect(screen.getByPlaceholderText("Paste GitHub personal access token")).toBeInTheDocument();
    expect(screen.getByText(
      "Supports tokens beginning with github_pat_ or ghp_. Stored securely in the system credential store.",
    )).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "GitLab" }));
    expect(screen.getByPlaceholderText("Paste GitLab personal access token")).toBeInTheDocument();
    expect(screen.getByText(
      "Supports tokens beginning with glpat-. Stored securely in the system credential store.",
    )).toBeInTheDocument();

    english.unmount();
    setLang("zh");
    render(
      <ToastProvider>
        <SettingsPanel onClose={vi.fn()} initialSection="github" />
      </ToastProvider>,
    );
    await screen.findByText("未配置");
    expect(screen.getByPlaceholderText("粘贴 GitHub Personal Access Token")).toBeInTheDocument();
    expect(screen.getByText(
      "支持以 github_pat_ 或 ghp_ 开头的令牌。凭据将安全存储于系统凭据库中。",
    )).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "GitLab" }));
    expect(screen.getByPlaceholderText("粘贴 GitLab Personal Access Token")).toBeInTheDocument();
    expect(screen.getByText(
      "支持以 glpat- 开头的令牌。凭据将安全存储于系统凭据库中。",
    )).toBeInTheDocument();
  });

  it("shows GitHub minimum permissions and opens the fine-grained token page", async () => {
    const user = userEvent.setup();
    renderPanel({ initialSection: "github" });
    await screen.findByText("Recommended minimum permissions");

    expect(screen.getByText("Pull requests")).toBeInTheDocument();
    expect(screen.getByText("Commit statuses")).toBeInTheDocument();
    expect(screen.getByText("Read and write")).toBeInTheDocument();
    expect(screen.getByText("Read-only")).toBeInTheDocument();
    expect(screen.getByText(/may not offer the Checks permission/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Create token on GitHub" }));
    expect(opener.openUrl).toHaveBeenCalledWith(
      "https://github.com/settings/personal-access-tokens/new",
    );
  });

  it("shows only a right-aligned Save credential action for an unconfigured provider", async () => {
    const user = userEvent.setup();
    renderPanel({ initialSection: "deepseek" });
    await screen.findByText("Not configured");

    const save = screen.getByRole("button", { name: "Save credential" });
    const actionBar = screen.getByTestId("settings-action-bar");
    expect(save).toBeDisabled();
    expect(actionBar).toBeInTheDocument();
    expect(actionBar).toHaveClass("shrink-0", "border-t", "bg-canvas");
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
    expect(input).toHaveAttribute("placeholder", "Enter a new credential to replace the saved credential");
    expect(screen.getByText(
      "Supports tokens beginning with github_pat_ or ghp_. Stored securely in the system credential store.",
    )).toBeInTheDocument();
    expect(screen.getByText("Authenticates private repositories, pull requests, and review publishing.")).toBeInTheDocument();
    const save = screen.getByRole("button", { name: "Save replacement" });
    const test = screen.getByRole("button", { name: "Test connection" });
    const remove = screen.getByRole("button", { name: "Remove credential" });
    const actions = screen.getByTestId("settings-action-bar");
    expect(within(actions).getAllByRole("button")).toEqual([remove, test, save]);
    expect(actions).toHaveClass("flex-col", "min-[441px]:flex-row");
    expect(remove).not.toHaveClass("min-[441px]:order-1");
    expect(test.parentElement).toHaveClass("min-[441px]:ml-auto", "min-[441px]:flex-row");
    expect(save).not.toHaveClass("min-[441px]:order-3");
    expect(save).toBeDisabled();

    await user.type(input, "replacement");
    expect(save).toBeEnabled();
    expect(document.body).not.toHaveTextContent("stored-secret");
  });

  it("renders the exact provider copy and replacement placeholder for every provider", async () => {
    ipc.credentialStatus.mockResolvedValue(true);
    const user = userEvent.setup();
    renderPanel({ initialSection: "deepseek" });
    await screen.findByText("Configured");

    expect(screen.getByText("Powers AI-assisted pull request reviews.")).toBeInTheDocument();
    expect(screen.getByPlaceholderText("Enter a new credential to replace the saved credential")).toBeInTheDocument();
    expect(screen.getByText("When AI Review runs, selected PR patches and code excerpts read during analysis are sent to DeepSeek.")).toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: "GitHub" }));
    expect(screen.getByText("Authenticates private repositories, pull requests, and review publishing.")).toBeInTheDocument();
    expect(screen.getByPlaceholderText("Enter a new credential to replace the saved credential")).toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: "GitLab" }));
    expect(screen.getByText("Authenticates private repositories, merge requests, and review operations.")).toBeInTheDocument();
    expect(screen.getByPlaceholderText("Enter a new credential to replace the saved credential")).toBeInTheDocument();
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
    expect(within(details).getByText("When AI Review runs, selected PR patches and code excerpts read during analysis are sent to DeepSeek.")).toHaveClass("text-fg-muted");
    expect(details).not.toHaveClass("rounded-md", "border", "bg-elevated");
    const label = screen.getByText("DeepSeek API key");
    expect(label).toHaveClass("text-xs");
    expect(label).not.toHaveClass("uppercase", "tracking-wide");
    expect(screen.getByLabelText("DeepSeek API key")).toHaveClass("h-9", "field");
    expect(screen.getByText("Powers AI-assisted pull request reviews.")).toBeInTheDocument();
    expect(screen.getByText("Stored securely in the system credential store.")).toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: "GitHub" }));
    expect(screen.queryByRole("heading", { name: "Service details" })).not.toBeInTheDocument();
  });

  it("associates credential inputs with helper copy and the DeepSeek disclosure", async () => {
    const user = userEvent.setup();
    renderPanel({ initialSection: "deepseek" });
    await screen.findByText("Not configured");

    const deepseekInput = screen.getByLabelText("DeepSeek API key");
    const deepseekDescriptions = deepseekInput.getAttribute("aria-describedby")!.split(" ");
    expect(deepseekDescriptions).toHaveLength(2);
    expect(document.getElementById(deepseekDescriptions[0])).toHaveTextContent(
      "Stored securely in the system credential store.",
    );
    expect(document.getElementById(deepseekDescriptions[1])).toHaveTextContent(
      "When AI Review runs, selected PR patches and code excerpts read during analysis are sent to DeepSeek.",
    );

    await user.click(screen.getByRole("tab", { name: "GitHub" }));
    const githubInput = screen.getByLabelText("GitHub personal access token");
    const githubDescriptions = githubInput.getAttribute("aria-describedby")!.split(" ");
    expect(githubDescriptions).toHaveLength(1);
    expect(document.getElementById(githubDescriptions[0])).toHaveTextContent(
      "Stored securely in the system credential store.",
    );
  });

  it("marks the active provider detail busy while statuses load and while an operation runs", async () => {
    let resolveStatuses!: (configured: boolean) => void;
    const statusPromise = new Promise<boolean>((resolve) => { resolveStatuses = resolve; });
    ipc.credentialStatus.mockImplementation(() => statusPromise);
    const user = userEvent.setup();
    renderPanel({ initialSection: "github" });
    const panel = screen.getByRole("tabpanel", { name: "GitHub" });
    expect(panel).toHaveAttribute("aria-busy", "true");

    resolveStatuses(true);
    await waitFor(() => expect(panel).toHaveAttribute("aria-busy", "false"));

    let resolveTest!: () => void;
    ipc.testCredential.mockImplementation(() => new Promise<void>((resolve) => { resolveTest = resolve; }));
    await user.click(screen.getByRole("button", { name: "Test connection" }));
    expect(panel).toHaveAttribute("aria-busy", "true");
    resolveTest();
    await waitFor(() => expect(panel).toHaveAttribute("aria-busy", "false"));
  });

  it("renders Chinese credential labels and configured actions", async () => {
    setLang("zh");
    ipc.credentialStatus.mockResolvedValue(true);
    const user = userEvent.setup();
    renderPanel({ initialSection: "github" });
    await screen.findByText("已配置");

    expect(screen.getByLabelText("GitHub 个人访问令牌")).toHaveValue("");
    expect(screen.getByRole("tablist", { name: "集成服务" })).toBeInTheDocument();
    expect(screen.getByText("管理 AI 评审与代码托管服务的凭据。")).toBeInTheDocument();
    expect(screen.getByText("用于私有仓库、拉取请求与评审发布的身份验证。")).toBeInTheDocument();
    expect(screen.getByPlaceholderText("输入新凭据以替换已保存的凭据")).toBeInTheDocument();
    expect(screen.getByText(
      "支持以 github_pat_ 或 ghp_ 开头的令牌。凭据将安全存储于系统凭据库中。",
    )).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "保存替换凭据" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "测试连接" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "移除凭据" })).toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: "DeepSeek" }));
    expect(screen.getByText("为 AI 拉取请求评审提供模型服务。")).toBeInTheDocument();
    expect(screen.getByPlaceholderText("输入新凭据以替换已保存的凭据")).toBeInTheDocument();
    expect(screen.getByText("运行 AI 评审时，所选 PR 补丁及分析过程中读取的代码摘录会发送至 DeepSeek。")).toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: "GitLab" }));
    expect(screen.getByText("用于私有仓库、合并请求与评审操作的身份验证。")).toBeInTheDocument();
    expect(screen.getByPlaceholderText("输入新凭据以替换已保存的凭据")).toBeInTheDocument();
  });

  it("uses one constrained vertical scroll owner for provider content", () => {
    renderPanel();

    const dialog = screen.getByRole("dialog", { name: "Settings" });
    const activePanel = screen.getByRole("tabpanel", { name: "DeepSeek" });
    const scrollOwner = activePanel.parentElement;
    const actionBar = screen.getByTestId("settings-action-bar");
    expect(scrollOwner).toHaveClass("min-h-0", "flex-1", "overflow-y-auto");
    expect(activePanel).not.toHaveClass("overflow-y-auto");
    expect(dialog.querySelectorAll(".overflow-y-auto")).toHaveLength(1);
    expect(actionBar.closest(".overflow-y-auto")).toBeNull();
  });

  it("resets provider content scroll when switching providers", async () => {
    const user = userEvent.setup();
    renderPanel({ initialSection: "deepseek" });
    await screen.findByText("Not configured");

    const scrollOwner = screen.getByRole("tabpanel", { name: "DeepSeek" }).parentElement!;
    scrollOwner.scrollTop = 240;

    await user.click(screen.getByRole("tab", { name: "GitHub" }));

    expect(scrollOwner.scrollTop).toBe(0);
    expect(screen.getByRole("tab", { name: "GitHub" })).toHaveFocus();
  });

  it("stacks categories above full-width content and keeps provider tabs scrollable on narrow screens", () => {
    renderPanel();

    const categories = screen.getByRole("navigation", { name: "Settings categories" });
    const layout = categories.parentElement;
    const providers = screen.getByRole("tablist", { name: "Integration" });
    expect(screen.getByRole("dialog", { name: "Settings" })).toHaveClass(
      "h-[min(680px,calc(100vh-48px))]",
      "w-[min(960px,calc(100vw-48px))]",
    );
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
    expect(screen.getByText("When AI Review runs, selected PR patches and code excerpts read during analysis are sent to DeepSeek.")).toBeInTheDocument();

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

  it("recovers a failed provider status after a successful credential save", async () => {
    ipc.credentialStatus.mockImplementation(async (kind: string) => {
      if (kind === "deepseek") throw { code: "STATUS_FAILED", message: "DeepSeek status unavailable" };
      return kind === "github";
    });
    renderPanel({ initialSection: "github" });

    expect(await screen.findByText("Configured")).toBeInTheDocument();
    expect(await screen.findByText("DeepSeek status unavailable")).toBeInTheDocument();
    const user = userEvent.setup();
    await user.click(screen.getByRole("tab", { name: "DeepSeek" }));
    expect(screen.getByText("Status unavailable")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save credential" })).toBeDisabled();
    await user.type(screen.getByLabelText("DeepSeek API key"), "recovery-key");
    expect(screen.getByRole("button", { name: "Save credential" })).toBeEnabled();
    await user.click(screen.getByRole("button", { name: "Save credential" }));

    expect(await screen.findByText("Configured")).toBeInTheDocument();
    expect(screen.queryByText("Status unavailable")).not.toBeInTheDocument();
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

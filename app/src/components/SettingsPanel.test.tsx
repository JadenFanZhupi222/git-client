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
    await user.click(screen.getByRole("button", { name: "Save" }));

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

    await user.click(screen.getByRole("button", { name: "Test" }));
    await waitFor(() => expect(ipc.testCredential).toHaveBeenCalledWith("github"));
    expect(await screen.findByText("GitHub credential is valid")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Clear" }));
    await waitFor(() => expect(ipc.clearCredential).toHaveBeenCalledWith("github"));
    expect(await screen.findByText("Not configured")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Test" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Clear" })).toBeDisabled();
  });

  it("shows the IpcError message in an error toast", async () => {
    const user = userEvent.setup();
    ipc.saveCredential.mockRejectedValue({ code: "SAVE_FAILED", message: "Credential store unavailable" });
    renderPanel({ initialSection: "deepseek" });
    await screen.findByText("Not configured");

    const input = screen.getByLabelText("DeepSeek API key");
    await user.type(input, "private-secret");
    await user.click(screen.getByRole("button", { name: "Save" }));

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
    expect(screen.getByRole("button", { name: "Save" })).toHaveFocus();
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
    await user.click(screen.getByRole("button", { name: "Test" }));
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
    await user.click(screen.getByRole("button", { name: "Save" }));

    const dialog = screen.getByRole("dialog", { name: "Settings" });
    expect(within(dialog).getByRole("button", { name: "Save" })).toBeDisabled();
    expect(within(dialog).getByRole("button", { name: "Test" })).toBeDisabled();
    expect(within(dialog).getByRole("button", { name: "Clear" })).toBeDisabled();
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
      await user.click(screen.getByRole("button", { name: operation === "clear" ? "Clear" : operation === "test" ? "Test" : "Save" }));
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

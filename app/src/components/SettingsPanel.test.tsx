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
});

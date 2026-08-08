import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ReviewModelOptionDto } from "../bindings";
import { setLang } from "../lib/i18n";
import { AgentModelPicker, CompatibleModelList } from "./AgentModelPicker";

const ipc = vi.hoisted(() => ({ credentialStatus: vi.fn() }));
vi.mock("../ipc", () => ipc);

const models: ReviewModelOptionDto[] = [
  model("deepseek-v4-flash", "DeepSeek V4 Flash", "DeepSeek", "deepseek", 1_000_000, 5_000_000),
  model("gpt-5.6-terra", "GPT-5.6 Terra", "OpenAI", "openai", 2_500_000, 15_000_000),
  model("claude-sonnet-5", "Claude Sonnet 5", "Anthropic", "anthropic", 2_000_000, 10_000_000),
];

describe("AgentModelPicker", () => {
  beforeEach(() => {
    setLang("en");
    ipc.credentialStatus.mockImplementation(async (kind: string) => kind === "deepseek");
  });

  it("groups compatible models, shows facts, and routes missing credentials to settings", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    const onConfigureCredential = vi.fn();
    const { rerender } = render(
      <AgentModelPicker
        id="agent-model"
        label="Model"
        models={models}
        value="deepseek-v4-flash"
        onChange={onChange}
        onConfigureCredential={onConfigureCredential}
      />,
    );

    const select = screen.getByRole("combobox", { name: "Model" });
    expect(Array.from(select.querySelectorAll("optgroup"), (group) => group.label)).toEqual([
      "DeepSeek",
      "OpenAI",
      "Anthropic",
    ]);
    expect(screen.getByText("128K context")).toBeInTheDocument();
    expect(screen.getByText("$1 in · $5 out / 1M")).toBeInTheDocument();
    await screen.findByText("Configured");

    await user.selectOptions(select, "gpt-5.6-terra");
    expect(onChange).toHaveBeenCalledWith("gpt-5.6-terra");
    rerender(
      <AgentModelPicker
        id="agent-model"
        label="Model"
        models={models}
        value="gpt-5.6-terra"
        onChange={onChange}
        onConfigureCredential={onConfigureCredential}
      />,
    );
    await screen.findByText("Not configured");
    expect(screen.getByText("$2.5 in · $15 out / 1M")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Configure OpenAI" }));
    expect(onConfigureCredential).toHaveBeenCalledWith("openai");
    await waitFor(() => expect(ipc.credentialStatus).toHaveBeenCalledTimes(3));
  });

  it("renders a flat compatible-model list without a model selector", () => {
    render(<CompatibleModelList models={models.slice(0, 2)} />);
    const list = screen.getByRole("list", { name: "Compatible agent models" });
    expect(within(list).getAllByRole("listitem")).toHaveLength(2);
    expect(within(list).queryByRole("combobox")).not.toBeInTheDocument();
  });
});

function model(
  id: string,
  label: string,
  provider: string,
  provider_id: "deepseek" | "openai" | "anthropic",
  input: number,
  output: number,
): ReviewModelOptionDto {
  return {
    id,
    label,
    provider,
    provider_id,
    capabilities: {
      supports_tool_calling: true,
      supports_structured_output: true,
      context_window_tokens: 128_000,
      max_output_tokens: 32_000,
      reports_usage: true,
    },
    pricing: {
      currency: "USD",
      input_cache_hit_per_million_micros: input / 10,
      input_cache_miss_per_million_micros: input,
      output_per_million_micros: output,
      source_url: "https://example.test/pricing",
      source_version: "test",
      checked_at: "2026-08-08",
    },
  };
}

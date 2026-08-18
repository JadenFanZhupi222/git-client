import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentEventDto } from "../bindings";
import { useAgentStream } from "./useAgentStream";

const ipc = vi.hoisted(() => ({ onAgentEvent: vi.fn() }));
vi.mock("../ipc", () => ipc);

function Harness() {
  const stream = useAgentStream();
  return (
    <div>
      <button onClick={() => void stream.begin("run-1")}>begin</button>
      <button onClick={() => stream.finish("failed")}>fail</button>
      <output>{stream.stream?.attempts[0]?.text ?? "empty"}</output>
      <output aria-label="run status">{stream.stream?.runStatus ?? "none"}</output>
    </div>
  );
}

function delta(runId: string, sequence: number, text: string): AgentEventDto {
  return {
    run_id: runId,
    sequence,
    attempt_id: 1,
    event_type: "output_text_delta",
    provider_id: null,
    model_id: null,
    response_id: null,
    delta: text,
    artifact_type: null,
    artifact_field: null,
    artifact_index: null,
    call_id: null,
    tool_name: null,
    usage: null,
    error_code: null,
    will_retry: null,
  };
}

describe("useAgentStream", () => {
  beforeEach(() => vi.clearAllMocks());

  it("subscribes before use, filters other runs, and cleans up", async () => {
    let receive: ((event: AgentEventDto) => void) | undefined;
    const unsubscribe = vi.fn();
    ipc.onAgentEvent.mockImplementation(async (callback) => {
      receive = callback;
      return unsubscribe;
    });
    const user = userEvent.setup();
    const view = render(<Harness />);

    await user.click(screen.getByRole("button", { name: "begin" }));
    expect(ipc.onAgentEvent).toHaveBeenCalledOnce();
    act(() => receive?.(delta("other-run", 1, "ignored")));
    expect(screen.getByText("empty")).toBeInTheDocument();
    act(() => receive?.(delta("run-1", 1, "streamed")));
    expect(screen.getByText("streamed")).toBeInTheDocument();

    view.unmount();
    expect(unsubscribe).toHaveBeenCalledOnce();
  });

  it("disconnects and records an explicit terminal state", async () => {
    const unsubscribe = vi.fn();
    ipc.onAgentEvent.mockResolvedValue(unsubscribe);
    const user = userEvent.setup();
    render(<Harness />);

    await user.click(screen.getByRole("button", { name: "begin" }));
    expect(screen.getByLabelText("run status")).toHaveTextContent("active");
    await user.click(screen.getByRole("button", { name: "fail" }));
    expect(screen.getByLabelText("run status")).toHaveTextContent("failed");
    expect(unsubscribe).toHaveBeenCalledOnce();
  });
});

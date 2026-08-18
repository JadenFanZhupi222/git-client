import { useCallback, useEffect, useRef, useState } from "react";
import { onAgentEvent } from "../ipc";
import {
  createAgentStream,
  finishAgentStream,
  reduceAgentEvent,
  type AgentRunStatus,
  type AgentStreamState,
} from "../lib/agentStream";

export function useAgentStream() {
  const [stream, setStream] = useState<AgentStreamState | null>(null);
  const unsubscribeRef = useRef<(() => void) | null>(null);
  const generationRef = useRef(0);

  const disconnect = useCallback(() => {
    generationRef.current += 1;
    unsubscribeRef.current?.();
    unsubscribeRef.current = null;
  }, []);

  const finish = useCallback((status: Exclude<AgentRunStatus, "active">) => {
    disconnect();
    setStream((current) => current ? finishAgentStream(current, status) : current);
  }, [disconnect]);

  const begin = useCallback(async (runId: string) => {
    generationRef.current += 1;
    const generation = generationRef.current;
    unsubscribeRef.current?.();
    unsubscribeRef.current = null;
    setStream(createAgentStream(runId));

    let unsubscribe: () => void;
    try {
      unsubscribe = await onAgentEvent((event) => {
        if (event.run_id !== runId || generationRef.current !== generation) return;
        setStream((current) => current ? reduceAgentEvent(current, event) : current);
      });
    } catch {
      if (generationRef.current === generation) setStream(null);
      return;
    }
    if (generationRef.current !== generation) {
      unsubscribe();
      return;
    }
    unsubscribeRef.current = unsubscribe;
  }, []);

  const reset = useCallback(() => {
    disconnect();
    setStream(null);
  }, [disconnect]);

  useEffect(() => disconnect, [disconnect]);

  return { stream, begin, finish, reset };
}

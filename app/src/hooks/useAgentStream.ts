import { useCallback, useEffect, useRef, useState } from "react";
import { onAgentEvent } from "../ipc";
import { createAgentStream, reduceAgentEvent, type AgentStreamState } from "../lib/agentStream";

export function useAgentStream() {
  const [stream, setStream] = useState<AgentStreamState | null>(null);
  const unsubscribeRef = useRef<(() => void) | null>(null);
  const generationRef = useRef(0);

  const end = useCallback(() => {
    generationRef.current += 1;
    unsubscribeRef.current?.();
    unsubscribeRef.current = null;
  }, []);

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
    end();
    setStream(null);
  }, [end]);

  useEffect(() => end, [end]);

  return { stream, begin, end, reset };
}

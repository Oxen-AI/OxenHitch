import { useCallback, useState } from "react";

import { messageOf } from "./api";

/** Runs an async action, tracking whether it is in flight and what went wrong. */
export function useAction<Args extends unknown[]>(action: (...args: Args) => Promise<unknown>) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = useCallback(
    async (...args: Args) => {
      setBusy(true);
      setError(null);
      try {
        await action(...args);
        return true;
      } catch (problem) {
        setError(messageOf(problem));
        return false;
      } finally {
        setBusy(false);
      }
    },
    [action],
  );

  return { run, busy, error, clearError: () => setError(null) };
}

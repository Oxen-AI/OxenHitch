import { useEffect, useState } from "react";

import type { Api } from "./api";

/**
 * The last characters of the key already saved on this Mac for a host, or `null` while
 * looking and when there is none. An empty host looks nothing up.
 */
export function useSavedKey(api: Api, host: string): string | null {
  const [saved, setSaved] = useState<string | null>(null);
  useEffect(() => {
    let current = true;
    setSaved(null);
    if (host) {
      api.savedKey(host).then(
        (hint) => current && setSaved(hint),
        () => undefined,
      );
    }
    return () => {
      current = false;
    };
  }, [api, host]);
  return saved;
}

// The only place the UI talks to the Rust side. Shapes mirror `hitch-core`.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** How Claude Desktop relates to Oxen right now. */
export type Link = "not_configured" | "connected" | "paused" | "drifted";

export interface Status {
  claudeInstalled: boolean;
  claudeRunning: boolean;
  /** An administrator manages Claude's settings, so Hitch's are ignored. */
  managed: boolean;
  link: Link;
  /** Claude is running with settings older than Hitch's last change. */
  needsRestart: boolean;
  baseUrl: string;
  keyHint: string | null;
  /** Models shown in Claude, default first. Empty means Claude chooses. */
  models: string[];
}

export interface Model {
  id: string;
  name: string | null;
}

export type Check =
  | { state: "ok"; models: Model[] }
  | { state: "invalid_key" }
  | { state: "unreachable"; message: string }
  | { state: "failed"; message: string };

export interface Api {
  status(): Promise<Status>;
  /** Checks the saved key against the gateway. `null` when there is no key. */
  check(): Promise<Check | null>;
  /** Rejects with a sentence to show the user. */
  connect(input: { apiKey?: string; baseUrl?: string }): Promise<Status>;
  setRouting(enabled: boolean): Promise<Status>;
  setModels(models: string[]): Promise<Status>;
  remove(): Promise<Status>;
  restartClaude(): Promise<Status>;
  /** Resolves with the model that answered. */
  testMessage(): Promise<string>;
  autostart(): Promise<boolean>;
  setAutostart(enabled: boolean): Promise<boolean>;
  open(url: string): Promise<void>;
  resize(height: number): Promise<void>;
  hide(): Promise<void>;
  quit(): Promise<void>;
  /** Called when status changes outside the UI. Returns a function that stops listening. */
  onStatus(listener: (status: Status) => void): () => void;
}

export const tauriApi: Api = {
  status: () => invoke("status"),
  check: () => invoke("check_gateway"),
  connect: ({ apiKey, baseUrl }) => invoke("connect", { apiKey, baseUrl }),
  setRouting: (enabled) => invoke("set_routing", { enabled }),
  setModels: (models) => invoke("set_models", { models }),
  remove: () => invoke("remove"),
  restartClaude: () => invoke("restart_claude"),
  testMessage: () => invoke("test_message"),
  autostart: () => invoke("autostart"),
  setAutostart: (enabled) => invoke("set_autostart", { enabled }),
  open: (url) => invoke("open_url", { url }),
  resize: (height) => invoke("resize", { height }),
  hide: () => invoke("hide"),
  quit: () => invoke("quit"),
  onStatus: (listener) => {
    const stop = listen<Status>("status", (event) => listener(event.payload));
    return () => void stop.then((off) => off());
  },
};

export const isTauri = () => "__TAURI_INTERNALS__" in window;

/** Rust commands reject with a plain sentence; anything else gets a generic one. */
export function messageOf(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Something went wrong. Please try again.";
}

export const hostOf = (url: string) => url.replace(/^https?:\/\//, "").split("/")[0];

export const links = {
  apiKeys: "https://www.oxen.ai/settings/profile",
  claudeDownload: "https://claude.com/download",
};

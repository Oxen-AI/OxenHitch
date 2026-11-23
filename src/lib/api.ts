/**
 * | Stage | Mechanism | Tag | Ref |
 * | --- | --- | --- | --- |
 * | CONTRACT | Define the UI API and JSON shapes | [mandatory] | R1 |
 * | SELECT | Route calls to Tauri, Python, or browser fake | [mandatory] | R1 |
 * | ADAPT | Translate bridge calls and status events | [mandatory] | R1 |
 *
 * EARS guards: R1 WHEN Python desktop is active, the UI calls only pywebview's API.
 * R1 IF a desktop bridge is not ready, THEN return a display-safe error.
 * CLOSED: views do not call native APIs directly.
 * Preconditions: browser globals identify the active desktop bridge.
 * Failure modes: bridge methods reject with UI-displayable errors.
 */

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
  /** Models chosen to show in Claude, default first. Empty means all of Oxen's. */
  models: string[];
  /** Custom Claude Messages compatible endpoint; Save does not contact it. */
  customEndpoint?: boolean;
  modelCatalog?: Model[] | null;
  relayError?: string | null;
}

export interface Model {
  /** The name Oxen knows the model by. */
  id: string;
  name: string | null;
  /** Oxen's stable identifier, when it provides one. */
  uuid?: string | null;
  /** Who makes the model. */
  developer?: string | null;
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
  connect(input: { apiKey?: string; baseUrl?: string; customEndpoint?: boolean; modelIds?: string[] }): Promise<Status>;
  setRouting(enabled: boolean): Promise<Status>;
  setModels(models: string[]): Promise<Status>;
  remove(): Promise<Status>;
  restartClaude(): Promise<Status>;
  /** The last characters of the key already saved for a host, or `null` if none is. */
  savedKey(host: string): Promise<string | null>;
  /** Claude's own app icon as an image URL, or `null` when it cannot be read. */
  claudeIcon(): Promise<string | null>;
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
  savedKey: (host) => invoke("saved_key", { host }),
  claudeIcon: () => invoke("claude_icon"),
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

type PythonBridge = {
  status(): Promise<Status>;
  check(): Promise<Check | null>;
  connect(apiKey?: string, baseUrl?: string, customEndpoint?: boolean, modelIds?: string[]): Promise<Status>;
  set_routing(enabled: boolean): Promise<Status>;
  set_models(models: string[]): Promise<Status>;
  remove(): Promise<Status>;
  restart_claude(): Promise<Status>;
  saved_key(host: string): Promise<string | null>;
  claude_icon(): Promise<string | null>;
  test_message(): Promise<string>;
  autostart(): Promise<boolean>;
  set_autostart(enabled: boolean): Promise<boolean>;
  open_url(url: string): Promise<void>;
  resize(height: number): Promise<void>;
  hide(): Promise<void>;
  quit(): Promise<void>;
};

declare global {
  interface Window {
    pywebview?: { api: PythonBridge };
  }
}

const python = () => {
  if (!window.pywebview?.api) throw new Error("The Hitch Python bridge is not ready.");
  return window.pywebview.api;
};

/** API adapter exposed by the Python pywebview host. */
export const pythonApi: Api = {
  status: () => python().status(),
  check: () => python().check(),
  connect: ({ apiKey, baseUrl, customEndpoint, modelIds }) => python().connect(apiKey, baseUrl, customEndpoint, modelIds),
  setRouting: (enabled) => python().set_routing(enabled),
  setModels: (models) => python().set_models(models),
  remove: () => python().remove(),
  restartClaude: () => python().restart_claude(),
  savedKey: (host) => python().saved_key(host),
  claudeIcon: () => python().claude_icon(),
  testMessage: () => python().test_message(),
  autostart: () => python().autostart(),
  setAutostart: (enabled) => python().set_autostart(enabled),
  open: (url) => python().open_url(url),
  resize: (height) => python().resize(height),
  hide: () => python().hide(),
  quit: () => python().quit(),
  onStatus: (listener) => {
    const onStatus = (event: Event) => listener((event as CustomEvent<Status>).detail);
    window.addEventListener("hitch-status", onStatus);
    return () => window.removeEventListener("hitch-status", onStatus);
  },
};

export const isPythonDesktop = () => Boolean(window.pywebview?.api);

/** Wait for the Python host to inject its API before mounting desktop UI.
 *
 * Require: callback mounts the application with the Python API.
 * Guarantee: callback runs once after pywebview exposes its JS API.
 * Maintain: browser preview selection remains owned by the application entrypoint.
 * Assert: callers can register before or after pywebviewready without missing readiness.
 */
export function whenPythonApiReady(callback: () => void): () => void {
  if (isPythonDesktop()) {
    callback();
    return () => {};
  }

  window.addEventListener("pywebviewready", callback, { once: true });
  return () => window.removeEventListener("pywebviewready", callback);
}

/** Rust commands reject with a plain sentence; anything else gets a generic one. */
export function messageOf(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Something went wrong. Please try again.";
}

/**
 * What to show in the Host field. A standard Oxen gateway is shown as just its host,
 * because typing that host back produces the same address. Anything else is shown whole.
 */
export function hostLabel(baseUrl: string): string {
  const match = /^(https?):\/\/([^/]+)\/api\/ai$/.exec(baseUrl);
  if (!match) return baseUrl;
  const [, scheme, host] = match;
  return scheme === (isLocalHost(host) ? "http" : "https") ? host : baseUrl;
}

export const isLocalHost = (host: string) =>
  /^(localhost|0\.0\.0\.0|127\.[\d.]+|\[::1\])(:\d+)?$/.test(host);

export const hostOf = (url: string) => url.replace(/^https?:\/\//, "").split("/")[0];

export const links = {
  apiKeys: "https://www.oxen.ai/settings/profile",
  claudeDownload: "https://claude.com/download",
};

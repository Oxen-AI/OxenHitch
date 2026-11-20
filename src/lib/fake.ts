// A stand-in backend with the same behavior as the real one. Used by tests, and by
// `pnpm dev` in a browser so the UI can be worked on without the desktop shell.

import { hostOf, isLocalHost, type Api, type Check, type Model, type Status } from "./api";

/** What the real backend does with a typed host: fill in the scheme and gateway path. */
function toBaseUrl(input: string): string {
  const text = input.trim().replace(/\/+$/, "");
  if (text.includes("://")) return /^https?:\/\/[^/]+$/.test(text) ? `${text}/api/ai` : text;
  const [host, ...path] = text.split("/");
  const scheme = isLocalHost(host) ? "http" : "https";
  return `${scheme}://${host}${path.length ? `/${path.join("/")}` : "/api/ai"}`;
}

export const fakeModels: Model[] = [
  { id: "claude-opus-5-5", name: "Claude Opus 5.5" },
  { id: "claude-sonnet-5-5", name: "Claude Sonnet 5.5" },
  { id: "claude-haiku-5-5", name: "Claude Haiku 5.5" },
  { id: "llama-4-maverick", name: null },
];

export interface FakeOptions {
  status?: Partial<Status>;
  check?: Check;
  /** Keys already saved on this machine, as host -> last four characters. */
  savedKeys?: Record<string, string>;
  /** The saved key cannot be read (keychain denied, or the key is gone). */
  keyUnreadable?: boolean;
  /** Milliseconds each call takes, to see loading states. */
  latency?: number;
}

export const freshStatus: Status = {
  claudeInstalled: true,
  claudeRunning: true,
  managed: false,
  link: "not_configured",
  needsRestart: false,
  baseUrl: "https://hub.oxen.ai/api/ai",
  keyHint: null,
  models: [],
};

export const connectedStatus: Status = { ...freshStatus, link: "connected", keyHint: "1234" };

export function createFakeApi(options: FakeOptions = {}) {
  let status: Status = { ...freshStatus, ...options.status };
  let autostart = false;
  const check: Check = options.check ?? { state: "ok", models: fakeModels };
  const savedKeys = options.savedKeys ?? {};
  const listeners = new Set<(status: Status) => void>();
  const calls: string[] = [];

  const wait = () => new Promise((done) => setTimeout(done, options.latency ?? 0));
  const update = (change: Partial<Status>) => {
    status = { ...status, ...change };
    listeners.forEach((listener) => listener(status));
    return status;
  };
  const record = async (name: string) => {
    calls.push(name);
    await wait();
  };

  const api: Api = {
    async status() {
      return status;
    },
    async check() {
      await record("check");
      return status.keyHint && !options.keyUnreadable ? check : null;
    },
    async connect({ apiKey, baseUrl }) {
      await record(`connect:${apiKey ?? ""}:${baseUrl ?? ""}`);
      const key = apiKey?.trim();
      const saved = savedKeys[hostOf(baseUrl?.trim() ? toBaseUrl(baseUrl) : status.baseUrl)];
      if (!key && !status.keyHint && !saved) throw "Paste your Oxen API key to connect.";
      if (key === "bad") throw "Oxen didn't accept that key. Check it and try again.";
      return update({
        link: "connected",
        keyHint: key ? key.slice(-4) : (saved ?? status.keyHint),
        baseUrl: baseUrl?.trim() ? toBaseUrl(baseUrl) : status.baseUrl,
        needsRestart: status.claudeRunning,
      });
    },
    async setRouting(enabled) {
      await record(`setRouting:${enabled}`);
      return update({ link: enabled ? "connected" : "paused", needsRestart: status.claudeRunning });
    },
    async setModels(models) {
      await record(`setModels:${models.join(",")}`);
      return update({ models, needsRestart: status.claudeRunning });
    },
    async remove() {
      await record("remove");
      return update({ ...freshStatus, needsRestart: status.claudeRunning });
    },
    async restartClaude() {
      await record("restartClaude");
      return update({ claudeRunning: true, needsRestart: false });
    },
    async savedKey(host) {
      return savedKeys[hostOf(toBaseUrl(host))] ?? null;
    },
    async testMessage() {
      await record("testMessage");
      return status.models[0] ?? fakeModels[0].id;
    },
    async autostart() {
      return autostart;
    },
    async setAutostart(enabled) {
      calls.push(`setAutostart:${enabled}`);
      autostart = enabled;
      return autostart;
    },
    async open(url) {
      calls.push(`open:${url}`);
    },
    async resize() {},
    async hide() {
      calls.push("hide");
    },
    async quit() {
      calls.push("quit");
    },
    onStatus(listener) {
      listeners.add(listener);
      return () => void listeners.delete(listener);
    },
  };

  return { api, calls, update };
}

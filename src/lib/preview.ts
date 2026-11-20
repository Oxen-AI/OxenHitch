// Browser preview: `?state=connected` (or paused, drifted, restart, noclaude, badkey, offline)
// picks the starting state. No query starts at first run. `&view=models` (or connection)
// opens that screen.

import type { View } from "../App";
import type { Api } from "./api";
import { connectedStatus, createFakeApi, type FakeOptions } from "./fake";

const states: Record<string, FakeOptions> = {
  connected: { status: connectedStatus },
  restart: { status: { ...connectedStatus, needsRestart: true } },
  closed: { status: { ...connectedStatus, claudeRunning: false } },
  paused: { status: { ...connectedStatus, link: "paused" } },
  drifted: { status: { ...connectedStatus, link: "drifted" } },
  managed: { status: { ...connectedStatus, managed: true } },
  models: { status: { ...connectedStatus, models: ["claude-opus-5-5", "claude-haiku-5-5"] } },
  noclaude: { status: { claudeInstalled: false, claudeRunning: false } },
  badkey: { status: connectedStatus, check: { state: "invalid_key" } },
  offline: {
    status: connectedStatus,
    check: { state: "unreachable", message: "Could not reach hub.oxen.ai." },
  },
};

export function preview(): Api {
  const state = new URLSearchParams(location.search).get("state") ?? "";
  return createFakeApi({ latency: 500, ...states[state] }).api;
}

export function previewView(): View | undefined {
  const view = new URLSearchParams(location.search).get("view");
  return view === "models" || view === "connection" ? view : undefined;
}

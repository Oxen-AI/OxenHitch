import { useCallback, useEffect, useState } from "react";

import pkg from "../../package.json";
import type { Gateway } from "../App";
import { hostOf, type Api, type Status } from "../lib/api";
import { useAction } from "../lib/useAction";
import type { ConnectionFocus } from "./Connection";
import { Bubble, Chevron, Notice, OxenMark, Spinner, Switch } from "../components/ui";

type Tone = "on" | "pending" | "off" | "trouble";

/** The one sentence that says where things stand, and the one thing to do about it. */
function headline(status: Status, gateway: Gateway): { tone: Tone; title: string; detail: string } {
  if (status.link === "drifted") {
    return {
      tone: "off",
      title: "Claude's setup changed",
      detail: "Claude is no longer pointed at Oxen.",
    };
  }
  if (status.needsRestart) {
    return {
      tone: "pending",
      title: "Restart Claude to finish",
      detail:
        status.link === "paused"
          ? "Claude will go back to signing in with Anthropic."
          : "Claude picks up Oxen the next time it opens.",
    };
  }
  if (status.link === "paused") {
    return {
      tone: "off",
      title: "Claude is using Anthropic",
      detail: "Oxen is switched off. Your key is saved.",
    };
  }
  if (gateway.state === "no_key") {
    return {
      tone: "trouble",
      title: status.claudeRunning ? "Claude is running on Oxen" : "Claude is set to use Oxen",
      detail: "Hitch needs your key again to check on Oxen.",
    };
  }
  if (gateway.state === "invalid_key" || gateway.state === "unreachable" || gateway.state === "failed") {
    return {
      tone: "trouble",
      title: status.claudeRunning ? "Claude is running on Oxen" : "Claude is set to use Oxen",
      detail: "There is a problem reaching Oxen. See below.",
    };
  }
  // The host has its own row below, so it is not repeated here.
  return status.claudeRunning
    ? { tone: "on", title: "Claude is running on Oxen", detail: "Your chats go through Oxen." }
    : { tone: "on", title: "Claude is set to use Oxen", detail: "Open Claude to start chatting." };
}

export function Home(props: {
  api: Api;
  status: Status;
  gateway: Gateway;
  onRetry: () => void;
  onOpen: (view: "models" | "connection", field?: ConnectionFocus) => void;
}) {
  const { api, status, gateway } = props;
  const { tone, title, detail } = headline(status, gateway);
  const routing = useAction((enabled: boolean) => api.setRouting(enabled));
  const on = status.link === "connected";

  return (
    <main className="screen home">
      <section className="hero">
        <div className="hitch" data-tone={tone} aria-hidden="true">
          <div className="node">
            <Bubble />
          </div>
          <div className="wire">
            <span className="bead" />
          </div>
          <div className="node node-oxen">
            <OxenMark size={20} />
          </div>
        </div>
        <h1>{title}</h1>
        <p className="detail">{detail}</p>

        {status.link === "drifted" ? (
          <button
            type="button"
            className="button primary compact"
            disabled={routing.busy}
            onClick={() => void routing.run(true)}
          >
            {routing.busy ? <Spinner /> : null} Reconnect
          </button>
        ) : status.needsRestart ? (
          <RestartPrompt api={api} />
        ) : !status.claudeRunning && on ? (
          <RestartPrompt api={api} opening />
        ) : null}
        {routing.error && <Notice>{routing.error}</Notice>}
      </section>

      {status.managed && (
        <Notice tone="info">
          Your organization manages Claude's settings, so changes made here won't take effect.
        </Notice>
      )}

      <div className="group">
        <div className="row">
          <span className="row-label">Run Claude on Oxen</span>
          <Switch
            label="Run Claude on Oxen"
            checked={on}
            disabled={routing.busy}
            onChange={(enabled) => void routing.run(enabled)}
          />
        </div>
        <div className="row">
          <span className="row-label">Claude Desktop</span>
          <span className="row-value">
            <span className={`dot ${status.claudeRunning ? "dot-good" : ""}`} />
            {status.claudeRunning ? "Running" : "Not running"}
          </span>
        </div>
        <GatewayRow gateway={gateway} host={hostOf(status.baseUrl)} onRetry={props.onRetry} />
        <button type="button" className="row row-link" onClick={() => props.onOpen("models")}>
          <span className="row-label">Models</span>
          <span className="row-value">{modelSummary(status, gateway)}</span>
          <Chevron />
        </button>
        <button type="button" className="row row-link" onClick={() => props.onOpen("connection", "host")}>
          <span className="row-label">Host</span>
          <span className="row-value">{hostOf(status.baseUrl)}</span>
          <Chevron />
        </button>
        <button type="button" className="row row-link" onClick={() => props.onOpen("connection", "key")}>
          <span className="row-label">API key</span>
          <span className="row-value mono">{status.keyHint ? `••••${status.keyHint}` : "Not set"}</span>
          <Chevron />
        </button>
      </div>

      <div className="group">
        <AutostartRow api={api} />
      </div>

      <footer className="footer">
        <span>Hitch {pkg.version}</span>
        <button type="button" className="text-link quiet" onClick={() => void api.quit()}>
          Quit
        </button>
      </footer>
    </main>
  );
}

/**
 * Quits and reopens Claude for the user (or just opens it). Always their click, never
 * automatic: a restart can interrupt a chat or a task that is still running.
 */
export function RestartPrompt(props: { api: Api; note?: string; opening?: boolean }) {
  const restart = useAction(() => props.api.restartClaude());
  const label = props.opening ? "Open Claude" : "Restart Claude";
  const working = props.opening ? "Opening Claude…" : "Restarting Claude…";
  return (
    <div className="restart">
      {props.note && <p className="detail">{props.note}</p>}
      <button
        type="button"
        className={`button compact ${props.opening ? "secondary" : "primary"}`}
        disabled={restart.busy}
        onClick={() => void restart.run()}
      >
        {restart.busy ? (
          <>
            <Spinner /> {working}
          </>
        ) : (
          label
        )}
      </button>
      {restart.error && <Notice>{restart.error}</Notice>}
    </div>
  );
}

function GatewayRow({ gateway, host, onRetry }: { gateway: Gateway; host: string; onRetry: () => void }) {
  const view = (() => {
    switch (gateway.state) {
      case "checking":
        return { dot: "idle", text: "Checking…" };
      case "ok": {
        const count = gateway.models.length;
        return { dot: "good", text: `Online · ${count} ${count === 1 ? "model" : "models"}` };
      }
      case "no_key":
        return { dot: "warn", text: "Key needed" };
      case "invalid_key":
        return { dot: "bad", text: "Key not accepted" };
      case "unreachable":
        return { dot: "warn", text: `Can't reach ${host}` };
      case "failed":
        return { dot: "warn", text: "Not responding" };
    }
  })();
  const failed = gateway.state !== "ok" && gateway.state !== "checking" && gateway.state !== "no_key";

  return (
    <>
      <div className="row">
        <span className="row-label">Gateway</span>
        <span className="row-value">
          <span className={`dot dot-${view.dot}`} />
          {view.text}
        </span>
        {failed && (
          <button type="button" className="text-link" onClick={onRetry}>
            Retry
          </button>
        )}
      </div>
      {gateway.state === "no_key" && (
        <p className="row-help">Hitch couldn't read your saved key. Enter your key again under API key.</p>
      )}
      {gateway.state === "invalid_key" && (
        <p className="row-help">Oxen no longer accepts this key. Replace it under API key.</p>
      )}
      {gateway.state === "unreachable" && (
        <p className="row-help">Check your internet connection. Claude can't answer until this is back.</p>
      )}
      {gateway.state === "failed" && <p className="row-help">{gateway.message}</p>}
    </>
  );
}

function modelSummary(status: Status, gateway: Gateway) {
  const [first, ...rest] = status.models;
  if (!first) return gateway.state === "ok" ? `All ${gateway.models.length}` : "All";
  const known = gateway.state === "ok" ? gateway.models.find((model) => model.id === first) : undefined;
  const name = known?.name ?? first;
  return rest.length ? `${name} + ${rest.length}` : name;
}

function AutostartRow({ api }: { api: Api }) {
  const [enabled, setEnabled] = useState(false);
  useEffect(() => {
    void api.autostart().then(setEnabled);
  }, [api]);
  const change = useCallback(
    async (next: boolean) => setEnabled(await api.setAutostart(next)),
    [api],
  );
  const toggle = useAction(change);

  return (
    <>
      <div className="row">
        <span className="row-label">Open at login</span>
        <Switch label="Open at login" checked={enabled} disabled={toggle.busy} onChange={(next) => void toggle.run(next)} />
      </div>
      {toggle.error && <p className="row-help">{toggle.error}</p>}
    </>
  );
}

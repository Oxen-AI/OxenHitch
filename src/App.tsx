import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";

import type { Api, Check, Status } from "./lib/api";
import { Connection, type ConnectionFocus } from "./views/Connection";
import { Home } from "./views/Home";
import { Models } from "./views/Models";
import { NoClaude, Welcome } from "./views/Welcome";

export type View = "home" | "models" | "connection";

/** What we know about the gateway: still asking, no key to ask with, or the answer. */
export type Gateway = { state: "checking" } | { state: "no_key" } | { state: "untested" } | Check;

export default function App({ api, initialView = "home" }: { api: Api; initialView?: View }) {
  const [status, setStatus] = useState<Status | null>(null);
  const [gateway, setGateway] = useState<Gateway>({ state: "checking" });
  const [view, setView] = useState<View>(initialView);
  const [focus, setFocus] = useState<ConnectionFocus>("key");
  const frame = useRef<HTMLDivElement>(null);

  useEffect(() => {
    void api.status().then(setStatus);
    return api.onStatus(setStatus);
  }, [api]);

  const configured = status !== null && status.link !== "not_configured";

  // Someone who has just finished setting up is told where Hitch lives, once.
  const [justSetUp, setJustSetUp] = useState(false);
  const wasConfigured = useRef<boolean | null>(null);
  useEffect(() => {
    if (status === null) return;
    if (wasConfigured.current === false && configured) setJustSetUp(true);
    wasConfigured.current = configured;
  }, [status, configured]);
  const baseUrl = status?.baseUrl;
  const keyHint = status?.keyHint;

  const checkGateway = useCallback(async () => {
    setGateway({ state: "checking" });
    try {
      const result = await api.check();
      setGateway(result ?? { state: "no_key" });
    } catch {
      setGateway({ state: "failed", message: "The gateway could not be checked." });
    }
  }, [api]);

  // Check when set up, when the key or address changes, and each time the popover opens.
  useEffect(() => {
    if (!configured) return;
    if (status?.customEndpoint) {
      setGateway({ state: "untested" });
      return;
    }
    void checkGateway();
    window.addEventListener("focus", checkGateway);
    return () => window.removeEventListener("focus", checkGateway);
  }, [configured, baseUrl, keyHint, checkGateway, status?.customEndpoint]);

  // Once Hitch is no longer set up (after Remove), sub-screens have nothing to show.
  useEffect(() => {
    if (status && !configured) setView("home");
  }, [status, configured]);

  // The window is exactly as tall as its content.
  useLayoutEffect(() => {
    const element = frame.current;
    if (!element || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => void api.resize(Math.ceil(element.offsetHeight)));
    observer.observe(element);
    return () => observer.disconnect();
  }, [api]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") void api.hide();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [api]);

  return (
    <div className="frame" ref={frame}>
      {status && <Screen key={screenKey(status, view)} {...{ api, status, gateway, view, setView, focus, setFocus, checkGateway, justSetUp, setJustSetUp }} />}
    </div>
  );
}

function screenKey(status: Status, view: View) {
  if (status.link === "not_configured") return status.claudeInstalled ? "welcome" : "noclaude";
  return view;
}

function Screen(props: {
  api: Api;
  status: Status;
  gateway: Gateway;
  view: View;
  setView: (view: View) => void;
  focus: ConnectionFocus;
  setFocus: (focus: ConnectionFocus) => void;
  justSetUp: boolean;
  setJustSetUp: (value: boolean) => void;
  checkGateway: () => Promise<void>;
}) {
  const { api, status, gateway, view, setView } = props;
  const home = () => setView("home");

  if (status.link === "not_configured") {
    return status.claudeInstalled ? <Welcome api={api} status={status} /> : <NoClaude api={api} />;
  }
  if (view === "models") {
    return <Models api={api} status={status} gateway={gateway} onRefresh={props.checkGateway} onDone={home} />;
  }
  if (view === "connection") {
    return <Connection api={api} status={status} focus={props.focus} onDone={home} />;
  }
  return (
    <Home
      api={api}
      status={status}
      gateway={gateway}
      onRetry={props.checkGateway}
      newcomer={props.justSetUp}
      onWelcomed={() => props.setJustSetUp(false)}
      onOpen={(next, field) => {
        if (field) props.setFocus(field);
        setView(next);
      }}
    />
  );
}

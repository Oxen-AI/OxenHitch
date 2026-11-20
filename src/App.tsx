import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";

import type { Api, Check, Status } from "./lib/api";
import { Connection } from "./views/Connection";
import { Home } from "./views/Home";
import { Models } from "./views/Models";
import { NoClaude, Welcome } from "./views/Welcome";

type View = "home" | "models" | "connection";

/** What we know about the gateway: still asking, or the answer. */
export type Gateway = { state: "checking" } | Check;

export default function App({ api }: { api: Api }) {
  const [status, setStatus] = useState<Status | null>(null);
  const [gateway, setGateway] = useState<Gateway>({ state: "checking" });
  const [view, setView] = useState<View>("home");
  const frame = useRef<HTMLDivElement>(null);

  useEffect(() => {
    void api.status().then(setStatus);
    return api.onStatus(setStatus);
  }, [api]);

  const configured = status !== null && status.link !== "not_configured";
  const baseUrl = status?.baseUrl;
  const keyHint = status?.keyHint;

  const checkGateway = useCallback(async () => {
    setGateway({ state: "checking" });
    try {
      const result = await api.check();
      if (result) setGateway(result);
    } catch {
      setGateway({ state: "failed", message: "The gateway could not be checked." });
    }
  }, [api]);

  // Check when set up, when the key or address changes, and each time the popover opens.
  useEffect(() => {
    if (!configured) return;
    void checkGateway();
    window.addEventListener("focus", checkGateway);
    return () => window.removeEventListener("focus", checkGateway);
  }, [configured, baseUrl, keyHint, checkGateway]);

  useEffect(() => {
    if (!configured) setView("home");
  }, [configured]);

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
      {status && <Screen key={screenKey(status, view)} {...{ api, status, gateway, view, setView, checkGateway }} />}
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
  checkGateway: () => void;
}) {
  const { api, status, gateway, view, setView } = props;
  const home = () => setView("home");

  if (status.link === "not_configured") {
    return status.claudeInstalled ? <Welcome api={api} status={status} /> : <NoClaude api={api} />;
  }
  if (view === "models") {
    return <Models api={api} status={status} gateway={gateway} onDone={home} />;
  }
  if (view === "connection") {
    return <Connection api={api} status={status} onDone={home} />;
  }
  return (
    <Home
      api={api}
      status={status}
      gateway={gateway}
      onRetry={props.checkGateway}
      onOpen={setView}
    />
  );
}

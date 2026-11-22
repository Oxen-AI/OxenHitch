/**
 * | Stage | Mechanism | Tag | Ref |
 * | --- | --- | --- | --- |
 * | DETECT | Identify the active desktop bridge | [mandatory] | R1 |
 * | WAIT | Wait for Python bridge injection when Python shell is marked | [mandatory] | R1 |
 * | SELECT | Choose native API or browser preview | [mandatory] | R1 |
 * | RENDER | Mount the shared React application | [mandatory] | R1 |
 *
 * EARS guards: R1 WHEN the URL fragment marks the Python desktop, THEN wait for pywebviewready before mounting with pythonApi.
 * R1 WHEN no desktop shell is present, the fake preview API is used.
 * CLOSED: native bridge selection stays at the application entrypoint.
 * Preconditions: #root exists in index.html.
 * Failure modes: render errors are reported by React/runtime tooling.
 */

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import App from "./App";
import { isTauri, pythonApi, tauriApi, whenPythonApiReady } from "./lib/api";
import { preview, previewView } from "./lib/preview";
import "./styles.css";

/** Mount the shared application with the selected native or fake API.
 *
 * Require: root exists and the selected API is ready for calls.
 * Guarantee: React is mounted once with desktop or preview state.
 * Maintain: browser styling and preview state are applied only outside desktop shells.
 * Assert: Python callers invoke this after the pywebview API exists.
 */
function mount(api: typeof pythonApi, desktop: boolean): void {
  if (!desktop) document.documentElement.classList.add("browser");

  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <App api={api} initialView={desktop ? undefined : previewView()} />
    </StrictMode>,
  );
}

const pythonDesktop = new URLSearchParams(location.hash.slice(1)).get("desktop") === "python";
if (isTauri()) {
  mount(tauriApi, true);
} else if (pythonDesktop) {
  whenPythonApiReady(() => mount(pythonApi, true));
} else {
  mount(preview(), false);
}

/**
 * | Stage | Mechanism | Tag | Ref |
 * | --- | --- | --- | --- |
 * | FIXTURE | Provide browser bridge and URL inputs | [contract] | R1 |
 * | VERIFY | Check adapter routing and value redaction | [mandatory] | R1 |
 *
 * EARS guards: R1 WHEN a Python bridge event arrives, THEN call the registered listener.
 * CLOSED: no network or desktop runtime is started.
 * Preconditions: Vitest supplies the JSDOM window.
 * Failure modes: assertion failures identify adapter contract drift.
 */

import { describe, expect, it, vi } from "vitest";

import { pythonApi, hostLabel, type Status, whenPythonApiReady } from "./api";

describe("hostLabel", () => {
  it("shows just the host for a standard Oxen gateway", () => {
    expect(hostLabel("https://hub.oxen.ai/api/ai")).toBe("hub.oxen.ai");
    expect(hostLabel("http://localhost:3001/api/ai")).toBe("localhost:3001");
    expect(hostLabel("http://127.0.0.1:3001/api/ai")).toBe("127.0.0.1:3001");
  });

  it("shows the whole address when the host alone would not bring it back", () => {
    expect(hostLabel("https://proxy.example.com/oxen")).toBe("https://proxy.example.com/oxen");
    expect(hostLabel("http://hub.oxen.ai/api/ai")).toBe("http://hub.oxen.ai/api/ai");
    expect(hostLabel("https://localhost:3001/api/ai")).toBe("https://localhost:3001/api/ai");
  });
});

describe("pythonApi", () => {
  it("waits for a late pywebview API injection before mounting", () => {
    const callback = vi.fn();
    Object.defineProperty(window, "pywebview", { configurable: true, value: undefined });

    const stop = whenPythonApiReady(callback);
    expect(callback).not.toHaveBeenCalled();

    Object.defineProperty(window, "pywebview", {
      configurable: true,
      value: { api: { status: vi.fn() } },
    });
    window.dispatchEvent(new Event("pywebviewready"));
    stop();

    expect(callback).toHaveBeenCalledOnce();
  });

  it("calls readiness immediately if the Python API already exists", () => {
    const callback = vi.fn();
    Object.defineProperty(window, "pywebview", {
      configurable: true,
      value: { api: { status: vi.fn() } },
    });

    whenPythonApiReady(callback);

    expect(callback).toHaveBeenCalledOnce();
  });

  it("routes calls through the pywebview method names", async () => {
    const status: Status = {
      claudeInstalled: true,
      claudeRunning: false,
      managed: false,
      link: "not_configured",
      needsRestart: false,
      baseUrl: "https://hub.oxen.ai/api/ai",
      keyHint: null,
      models: [],
    };
    const connect = vi.fn(async () => status);
    Object.defineProperty(window, "pywebview", {
      configurable: true,
      value: { api: { connect, status: vi.fn(async () => status) } },
    });

    await pythonApi.connect({ apiKey: "entered-key", baseUrl: "localhost:3001" });

    expect(connect).toHaveBeenCalledWith("entered-key", "localhost:3001", undefined, undefined);
  });

  it("delivers Python status events to registered UI listeners", () => {
    const listener = vi.fn();
    const stop = pythonApi.onStatus(listener);
    const status: Status = {
      claudeInstalled: true,
      claudeRunning: true,
      managed: false,
      link: "connected",
      needsRestart: true,
      baseUrl: "https://hub.oxen.ai/api/ai",
      keyHint: "1234",
      models: [],
    };

    window.dispatchEvent(new CustomEvent("hitch-status", { detail: status }));
    stop();
    window.dispatchEvent(new CustomEvent("hitch-status", { detail: status }));

    expect(listener).toHaveBeenCalledTimes(1);
    expect(listener).toHaveBeenCalledWith(status);
  });
});

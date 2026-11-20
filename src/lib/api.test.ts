import { describe, expect, it } from "vitest";

import { hostLabel } from "./api";

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

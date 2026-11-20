import { describe, expect, it } from "vitest";

import { nextVersion, readVersions, setVersion } from "./version.mjs";

const files = {
  "package.json": '{\n  "name": "oxen-hitch",\n  "version": "0.1.0",\n  "type": "module"\n}\n',
  "src-tauri/tauri.conf.json": '{\n  "productName": "Hitch",\n  "version": "0.1.0",\n  "identifier": "ai.oxen.hitch"\n}\n',
  "src-tauri/Cargo.toml":
    '[package]\nname = "oxen-hitch"\nversion = "0.1.0"\nedition = "2021"\n\n[dependencies]\ntauri = { version = "2", features = [] }\n',
  "core/Cargo.toml": '[package]\nname = "hitch-core"\nversion = "0.1.0"\n\n[dependencies]\nserde = { version = "1" }\n',
  "Cargo.lock":
    '[[package]]\nname = "hitch-core"\nversion = "0.1.0"\ndependencies = [\n "serde",\n]\n\n[[package]]\nname = "oxen-hitch"\nversion = "0.1.0"\n\n[[package]]\nname = "serde"\nversion = "0.1.0"\n',
};

describe("nextVersion", () => {
  it("bumps the part asked for and resets the parts after it", () => {
    expect(nextVersion("0.1.0", "patch")).toBe("0.1.1");
    expect(nextVersion("0.1.9", "minor")).toBe("0.2.0");
    expect(nextVersion("0.9.9", "major")).toBe("1.0.0");
  });

  it("takes an exact version as given", () => {
    expect(nextVersion("0.1.0", "2.3.4")).toBe("2.3.4");
    expect(nextVersion("0.1.0", "v2.3.4")).toBe("2.3.4");
  });

  it("refuses anything else", () => {
    for (const bad of ["", "big", "1.2", "1.2.3.4", "1.2.x", "1.2.3-beta"]) {
      expect(() => nextVersion("0.1.0", bad), bad).toThrow();
    }
  });

  it("refuses to go backwards or stand still", () => {
    expect(() => nextVersion("0.2.0", "0.1.9")).toThrow(/newer/);
    expect(() => nextVersion("0.2.0", "0.2.0")).toThrow(/newer/);
  });
});

describe("readVersions", () => {
  it("finds the version in every file that carries one", () => {
    expect(readVersions(files)).toEqual({
      "package.json": "0.1.0",
      "src-tauri/tauri.conf.json": "0.1.0",
      "src-tauri/Cargo.toml": "0.1.0",
      "core/Cargo.toml": "0.1.0",
      "Cargo.lock (hitch-core)": "0.1.0",
      "Cargo.lock (oxen-hitch)": "0.1.0",
    });
  });
});

describe("setVersion", () => {
  it("changes our own versions and nothing else", () => {
    const after = setVersion(files, "0.2.0");

    expect(new Set(Object.values(readVersions(after)))).toEqual(new Set(["0.2.0"]));
    // A dependency that happens to share the old version number is left alone.
    expect(after["Cargo.lock"]).toContain('name = "serde"\nversion = "0.1.0"');
    expect(after["src-tauri/Cargo.toml"]).toContain('tauri = { version = "2"');
    expect(after["core/Cargo.toml"]).toContain('serde = { version = "1" }');
    // Only the version lines differ.
    for (const [name, text] of Object.entries(files)) {
      expect(after[name].replaceAll("0.2.0", "0.1.0"), name).toBe(text);
    }
  });

  it("fails loudly if a file has no version to change", () => {
    expect(() => setVersion({ ...files, "package.json": "{}\n" }, "0.2.0")).toThrow(/package\.json/);
  });
});

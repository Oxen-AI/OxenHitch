// Hitch's version lives in several files that must always agree: the web package, the
// Tauri config (which names the app and its installer), both Rust crates, and the lock
// file. This script reads them, checks them, and bumps them together.
//
//   node scripts/version.mjs check            print the version; fail if the files disagree
//   node scripts/version.mjs check v0.2.0     also fail if it is not this version
//   node scripts/version.mjs bump patch       or minor, major, or an exact version like 0.2.0

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const OUR_CRATES = ["hitch-core", "oxen-hitch"];

const jsonVersion = /^(\s*"version":\s*")([^"]+)(")/m;
const cargoVersion = /^(\[package\][\s\S]*?^version\s*=\s*")([^"]+)(")/m;
const lockVersion = (crate) => new RegExp(`(name = "${crate}"\\nversion = ")([^"]+)(")`);

/** Where each version is: a label for messages, the file it is in, and how to find it. */
const PLACES = [
  { label: "package.json", file: "package.json", pattern: jsonVersion },
  { label: "src-tauri/tauri.conf.json", file: "src-tauri/tauri.conf.json", pattern: jsonVersion },
  { label: "src-tauri/Cargo.toml", file: "src-tauri/Cargo.toml", pattern: cargoVersion },
  { label: "core/Cargo.toml", file: "core/Cargo.toml", pattern: cargoVersion },
  ...OUR_CRATES.map((crate) => ({
    label: `Cargo.lock (${crate})`,
    file: "Cargo.lock",
    pattern: lockVersion(crate),
  })),
];

export const FILES = [...new Set(PLACES.map((place) => place.file))];

const parse = (version) => {
  const match = /^v?(\d+)\.(\d+)\.(\d+)$/.exec(version);
  return match ? match.slice(1).map(Number) : null;
};

/** The version after a bump of `patch`, `minor` or `major`, or the exact version given. */
export function nextVersion(current, change) {
  const [major, minor, patch] = parse(current) ?? [];
  if (major === undefined) throw new Error(`The current version "${current}" is not like 1.2.3.`);

  const bumped = {
    major: [major + 1, 0, 0],
    minor: [major, minor + 1, 0],
    patch: [major, minor, patch + 1],
  }[change];
  const next = bumped ?? parse(change);
  if (!next) throw new Error(`"${change}" is not patch, minor, major, or a version like 1.2.3.`);

  const now = [major, minor, patch];
  const firstDifference = next.findIndex((part, index) => part !== now[index]);
  const newer = firstDifference !== -1 && next[firstDifference] > now[firstDifference];
  if (!newer) throw new Error(`${next.join(".")} is not newer than ${current}.`);
  return next.join(".");
}

/** Every place's version, by label. `files` maps a file's path to its text. */
export function readVersions(files) {
  return Object.fromEntries(
    PLACES.map(({ label, file, pattern }) => {
      const match = pattern.exec(files[file] ?? "");
      if (!match) throw new Error(`No version found in ${label}.`);
      return [label, match[2]];
    }),
  );
}

/** The same files with every one of our versions set to `version`. */
export function setVersion(files, version) {
  const after = { ...files };
  for (const { label, file, pattern } of PLACES) {
    if (!pattern.test(after[file] ?? "")) throw new Error(`No version found in ${label}.`);
    after[file] = after[file].replace(pattern, `$1${version}$3`);
  }
  return after;
}

function main([command, argument]) {
  const root = join(dirname(fileURLToPath(import.meta.url)), "..");
  const files = Object.fromEntries(FILES.map((file) => [file, readFileSync(join(root, file), "utf8")]));
  const versions = readVersions(files);
  const distinct = [...new Set(Object.values(versions))];

  if (distinct.length !== 1) {
    const list = Object.entries(versions).map(([label, version]) => `  ${version}  ${label}`);
    throw new Error(`The version is not the same everywhere:\n${list.join("\n")}`);
  }
  const [current] = distinct;

  if (command === "check") {
    const expected = argument?.replace(/^v/, "");
    if (expected && expected !== current) {
      throw new Error(`The files say ${current}, but ${argument} was expected.`);
    }
    console.log(current);
  } else if (command === "bump") {
    const next = nextVersion(current, argument ?? "");
    const after = setVersion(files, next);
    for (const file of FILES) writeFileSync(join(root, file), after[file]);
    console.log(next);
  } else {
    throw new Error("Usage: node scripts/version.mjs check [version] | bump <patch|minor|major|x.y.z>");
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    main(process.argv.slice(2));
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }
}

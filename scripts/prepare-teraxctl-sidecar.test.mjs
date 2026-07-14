import assert from "node:assert/strict";
import path from "node:path";
import test from "node:test";

import {
  cargoBuildEnvironment,
  parseHostTriple,
  parseProfile,
  sidecarDestination,
} from "./prepare-teraxctl-sidecar.mjs";

test("disables sidecar validation only for the bootstrap cargo build", () => {
  const environment = cargoBuildEnvironment({
    KEEP_ME: "yes",
    TAURI_CONFIG: JSON.stringify({ build: { features: ["existing"] } }),
  });
  const config = JSON.parse(environment.TAURI_CONFIG);

  assert.equal(environment.KEEP_ME, "yes");
  assert.deepEqual(config.build, { features: ["existing"] });
  assert.deepEqual(config.bundle.externalBin, []);
});

test("parses rustc host and stages Windows suffix", () => {
  const triple = parseHostTriple(
    "release: 1.88.0\nhost: x86_64-pc-windows-msvc\n",
  );

  assert.equal(triple, "x86_64-pc-windows-msvc");
  assert.equal(
    sidecarDestination(triple, "C:\\repo"),
    path.join(
      "C:\\repo",
      "src-tauri",
      "binaries",
      "teraxctl-x86_64-pc-windows-msvc.exe",
    ),
  );
});

test("rejects rustc output without one host line", () => {
  assert.throws(() => parseHostTriple("release: 1.88.0\n"), /host/i);
  assert.throws(
    () => parseHostTriple("host: first\nhost: second\n"),
    /host/i,
  );
});

test("accepts exactly one supported profile flag", () => {
  assert.equal(parseProfile(["--debug"]), "debug");
  assert.equal(parseProfile(["--release"]), "release");
  assert.throws(() => parseProfile([]), /--debug.*--release/i);
  assert.throws(() => parseProfile(["--debug", "extra"]), /--debug.*--release/i);
  assert.throws(() => parseProfile(["--profile", "release"]), /--debug.*--release/i);
});

import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptPath = fileURLToPath(import.meta.url);
const defaultRepoRoot = path.resolve(path.dirname(scriptPath), "..");

export function parseHostTriple(versionOutput) {
  const hosts = String(versionOutput)
    .split(/\r?\n/u)
    .map((line) => /^host:\s*(\S+)\s*$/u.exec(line)?.[1])
    .filter(Boolean);

  if (hosts.length !== 1 || !/^[a-zA-Z0-9_.-]+$/u.test(hosts[0])) {
    throw new Error("rustc -vV must contain exactly one valid host line");
  }
  return hosts[0];
}

export function parseProfile(args) {
  if (args.length === 1 && args[0] === "--debug") return "debug";
  if (args.length === 1 && args[0] === "--release") return "release";
  throw new Error("expected exactly --debug or --release");
}

export function sidecarDestination(triple, repoRoot = defaultRepoRoot) {
  if (!/^[a-zA-Z0-9_.-]+$/u.test(triple)) {
    throw new Error("invalid Rust host triple");
  }
  return path.join(
    repoRoot,
    "src-tauri",
    "binaries",
    `teraxctl-${triple}.exe`,
  );
}

export function cargoBuildEnvironment(baseEnvironment) {
  let inlineConfig = {};
  if (baseEnvironment.TAURI_CONFIG) {
    try {
      inlineConfig = JSON.parse(baseEnvironment.TAURI_CONFIG);
    } catch {
      throw new Error("TAURI_CONFIG must contain valid inline JSON");
    }
  }

  return {
    ...baseEnvironment,
    TAURI_CONFIG: JSON.stringify({
      ...inlineConfig,
      bundle: {
        ...inlineConfig.bundle,
        externalBin: [],
      },
    }),
  };
}

function stageSidecar() {
  if (process.platform !== "win32") {
    throw new Error("teraxctl sidecar staging is supported only on Windows");
  }

  const profile = parseProfile(process.argv.slice(2));
  const triple = parseHostTriple(
    execFileSync("rustc", ["-vV"], { encoding: "utf8" }),
  );
  const manifest = path.join(defaultRepoRoot, "src-tauri", "Cargo.toml");
  const cargoArgs = ["build", "--manifest-path", manifest, "--bin", "teraxctl"];
  if (profile === "release") cargoArgs.push("--release");

  execFileSync("cargo", cargoArgs, {
    cwd: defaultRepoRoot,
    env: cargoBuildEnvironment(process.env),
    stdio: ["ignore", "ignore", "inherit"],
  });

  const source = path.join(
    defaultRepoRoot,
    "src-tauri",
    "target",
    profile,
    "teraxctl.exe",
  );
  const destination = sidecarDestination(triple);
  mkdirSync(path.dirname(destination), { recursive: true });
  copyFileSync(source, destination);
  process.stdout.write(`${destination}\n`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === scriptPath) {
  try {
    stageSidecar();
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    process.stderr.write(`${message}\n`);
    process.exitCode = 1;
  }
}

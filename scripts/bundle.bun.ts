#!/usr/bin/env bun
/**
 * Build target/Keytap.app with a stable codesign identifier for TCC.
 *
 *   bun scripts/bundle.bun.ts [--release|--debug] [--bin path]
 *
 * Prefers an Apple Development identity when `security find-identity` lists
 * one; otherwise ad-hoc signs with --identifier dev.keytap. Ad-hoc is enough
 * for a stable Accessibility identity on this machine; a Developer ID would
 * be needed only for distribution outside this Mac.
 */

const BUNDLE_ID = "dev.keytap";
const repoRoot = Bun.fileURLToPath(new URL("..", import.meta.url)).replace(/\/$/, "");
const appPath = `${repoRoot}/target/Keytap.app`;
const contents = `${appPath}/Contents`;
const macos = `${contents}/MacOS`;
const infoSrc = `${repoRoot}/packaging/macos/Info.plist`;

const args = process.argv.slice(2);
let profile: "release" | "debug" = "release";
let binOverride: string | null = null;
for (let i = 0; i < args.length; i++) {
  if (args[i] === "--release") profile = "release";
  else if (args[i] === "--debug") profile = "debug";
  else if (args[i] === "--bin") binOverride = args[++i] ?? null;
  else if (args[i] === "--help" || args[i] === "-h") {
    console.error(
      "Usage: bun scripts/bundle.bun.ts [--release|--debug] [--bin path]",
    );
    process.exit(2);
  } else {
    console.error(`unknown argument ${args[i]}`);
    process.exit(2);
  }
}

async function resolveBin(): Promise<string> {
  if (binOverride) return binOverride;
  const built = `${repoRoot}/target/${profile}/keytap`;
  if (!(await Bun.file(built).exists())) {
    console.log(`building keytap (${profile})...`);
    const flags = profile === "release" ? ["--release"] : [];
    const r = await Bun.$`cargo build -p keytap ${flags}`.cwd(repoRoot);
    if (r.exitCode !== 0) process.exit(r.exitCode ?? 1);
  }
  if (!(await Bun.file(built).exists())) {
    console.error(`binary missing: ${built}`);
    process.exit(1);
  }
  return built;
}

/** Pick "Apple Development: ..." if present; else null for ad-hoc. */
async function pickIdentity(): Promise<string | null> {
  const out = await Bun.$`security find-identity -p codesigning -v`.nothrow().text();
  for (const line of out.split("\n")) {
    //  1) HASH "Apple Development: Name (TEAM)"
    const m = line.match(/^\s*\d+\)\s+[A-F0-9]+\s+"(Apple Development:[^"]+)"/);
    if (m) return m[1] ?? null;
  }
  return null;
}

const bin = await resolveBin();
await Bun.$`mkdir -p ${macos}`;
await Bun.$`cp ${bin} ${macos}/keytap`;
await Bun.$`chmod +x ${macos}/keytap`;
await Bun.$`cp ${infoSrc} ${contents}/Info.plist`;

const identity = await pickIdentity();
if (identity) {
  console.log(`codesign identity: ${identity}`);
  // Real identity: identifier comes from the signed blob; still set it explicitly.
  const r =
    await Bun.$`codesign --force --sign ${identity} --identifier ${BUNDLE_ID} ${appPath}`.nothrow();
  if (r.exitCode !== 0) {
    console.error("codesign with Apple Development failed");
    process.exit(1);
  }
} else {
  console.log(
    "codesign identity: ad-hoc (-) with --identifier dev.keytap (no Apple Development identity found)",
  );
  const r =
    await Bun.$`codesign --force --sign - --identifier ${BUNDLE_ID} ${appPath}`.nothrow();
  if (r.exitCode !== 0) {
    console.error("ad-hoc codesign failed");
    process.exit(1);
  }
}

console.log(`bundled ${appPath}`);
console.log(`executable ${macos}/keytap`);

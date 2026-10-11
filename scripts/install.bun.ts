#!/usr/bin/env bun
/**
 * Install or uninstall the keytap launch agent.
 *
 *   bun scripts/install.bun.ts install [--bin path/to/keytap]
 *   bun scripts/install.bun.ts uninstall
 *
 * Resolves paths at install time. Never writes machine-local paths into git.
 */

const LABEL = "dev.keytap";
const home = Bun.env.HOME;
if (!home) {
  console.error("HOME is not set");
  process.exit(1);
}

const launchAgents = `${home}/Library/LaunchAgents`;
const plistPath = `${launchAgents}/${LABEL}.plist`;
const logPath = `${home}/Library/Logs/keytap.log`;

const repoRoot = Bun.fileURLToPath(new URL("..", import.meta.url));
const templatePath = `${repoRoot}/packaging/launchd/dev.keytap.plist.in`;

const args = process.argv.slice(2);
const cmd = args[0];

function usage(): never {
  console.error(`Usage:
  bun scripts/install.bun.ts install [--bin <path>]
  bun scripts/install.bun.ts uninstall`);
  process.exit(2);
}

async function defaultBin(): Promise<string> {
  const release = `${repoRoot}/target/release/keytap`;
  const debug = `${repoRoot}/target/debug/keytap`;
  if (await Bun.file(release).exists()) return release;
  if (await Bun.file(debug).exists()) return debug;
  console.error("No keytap binary found. Build first: cargo build -p keytap --release");
  process.exit(1);
}

async function install() {
  let bin: string | null = null;
  for (let i = 1; i < args.length; i++) {
    if (args[i] === "--bin") {
      bin = args[++i] ?? null;
    } else {
      usage();
    }
  }
  bin = bin ?? (await defaultBin());
  const binFile = Bun.file(bin);
  if (!(await binFile.exists())) {
    console.error(`binary not found: ${bin}`);
    process.exit(1);
  }

  const template = await Bun.file(templatePath).text();
  const rendered = template
    .replaceAll("@KEYTAP_BIN@", bin)
    .replaceAll("@KEYTAP_LOG@", logPath);

  await Bun.$`mkdir -p ${launchAgents}`.quiet();
  await Bun.$`mkdir -p ${home}/Library/Logs`.quiet();
  await Bun.write(plistPath, rendered);

  // Unload if present, then load.
  await Bun.$`launchctl bootout gui/$UID/${LABEL}`.nothrow().quiet();
  const load = await Bun.$`launchctl bootstrap gui/$UID ${plistPath}`.nothrow();
  if (load.exitCode !== 0) {
    // Fallback for older launchctl.
    await Bun.$`launchctl unload ${plistPath}`.nothrow().quiet();
    const legacy = await Bun.$`launchctl load ${plistPath}`.nothrow();
    if (legacy.exitCode !== 0) {
      console.error("launchctl load failed; plist written to", plistPath);
      process.exit(1);
    }
  }
  console.log("installed", plistPath);
  console.log("binary  ", bin);
  console.log("log     ", logPath);
  console.log("Grant Accessibility to the keytap binary if prompted, then:");
  console.log(`  launchctl kickstart -k gui/$UID/${LABEL}`);
}

async function uninstall() {
  await Bun.$`launchctl bootout gui/$UID/${LABEL}`.nothrow().quiet();
  await Bun.$`launchctl unload ${plistPath}`.nothrow().quiet();
  try {
    await Bun.$`rm -f ${plistPath}`;
  } catch {
    /* absent is fine */
  }
  console.log("uninstalled", LABEL);
}

if (cmd === "install") await install();
else if (cmd === "uninstall") await uninstall();
else usage();

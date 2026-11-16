#!/usr/bin/env bun
/**
 * Install or uninstall the keytap launch agent.
 *
 *   bun scripts/bundle.bun.ts
 *   bun scripts/install.bun.ts install
 *   bun scripts/install.bun.ts uninstall
 *
 * Points launchd at Keytap.app/Contents/MacOS/keytap so Accessibility TCC
 * uses the bundle id (dev.keytap), not the terminal. Paths are resolved at
 * install time; never commit a rendered plist.
 */

import { constants as fsConstants } from "node:fs";
import { randomUUID } from "node:crypto";
import { access, lstat, mkdir, rename, rm, rmdir } from "node:fs/promises";
import { userInfo } from "node:os";
import { dirname, resolve } from "node:path";

type InstallShellCommand = {
  quiet(): PromiseLike<{ exitCode: number }>;
  nothrow(): InstallShellCommand;
};

export type InstallIO = {
  userInfo(): { homedir: string };
  mkdir(path: string, options: { recursive?: boolean; mode?: number }): Promise<unknown>;
  rmdir(path: string): Promise<void>;
  file(path: string): { exists(): Promise<boolean>; text(): Promise<string> };
  write(path: string, content: string): PromiseLike<unknown>;
  shell(strings: TemplateStringsArray, ...values: string[]): InstallShellCommand;
  installBundle(): Promise<void>;
};

const LABEL = "dev.keytap";
const repoRoot = Bun.fileURLToPath(new URL("..", import.meta.url)).replace(/\/$/, "");
const templatePath = `${repoRoot}/packaging/launchd/dev.keytap.plist.in`;
const builtApp = `${repoRoot}/target/Keytap.app`;
// Installed copy lives outside target/ so cargo clean or disk purges cannot
// strand the launch agent. Copy preserves the signature (same cdhash).
const installedApp = "/Applications/Keytap.app";
const defaultAppBin = `${installedApp}/Contents/MacOS/keytap`;

function usage(): never {
  console.error(`Usage:
  bun scripts/install.bun.ts install [--bin <Keytap.app/Contents/MacOS/keytap>]
  bun scripts/install.bun.ts uninstall`);
  process.exit(2);
}

function xmlEscape(value: string): string {
  return value.replace(/[&<>"']/g, (character) => {
    switch (character) {
      case "&": return "&amp;";
      case "<": return "&lt;";
      case ">": return "&gt;";
      case "\"": return "&quot;";
      case "'": return "&apos;";
      default: return character;
    }
  });
}

function validatePlistPath(field: string, value: string): void {
  for (let index = 0; index < value.length; index++) {
    const first = value.charCodeAt(index);
    let codePoint = first;

    if (first >= 0xd800 && first <= 0xdbff) {
      const second = value.charCodeAt(index + 1);
      if (second < 0xdc00 || second > 0xdfff) {
        codePoint = first;
      } else {
        codePoint = 0x10000 + ((first - 0xd800) << 10) + (second - 0xdc00);
        index++;
      }
    }

    const xml10Character =
      codePoint === 0x9 ||
      codePoint === 0xa ||
      (codePoint >= 0x20 && codePoint <= 0xd7ff) ||
      (codePoint >= 0xe000 && codePoint <= 0xfffd) ||
      (codePoint >= 0x10000 && codePoint <= 0x10ffff);
    if (!xml10Character || codePoint === 0xd) {
      console.error(`${field} contains a character that cannot be represented exactly in the launchd plist`);
      process.exit(2);
    }
  }
}

export class LockHeldError extends Error {
  readonly code = "EEXIST";

  constructor(lockPath: string) {
    super(
      `another install is running or a stale lock exists at ${lockPath}; remove it if no installer is running`,
    );
    this.name = "LockHeldError";
  }
}

function rethrowLockError(error: unknown, lockPath: string): never {
  if ((error as NodeJS.ErrnoException).code === "EEXIST") {
    throw new LockHeldError(lockPath);
  }
  throw error;
}

async function installBundle(): Promise<void> {
  const installParent = dirname(installedApp);
  const lockPath = `${installedApp}.install-lock`;
  let lockAcquired = false;
  let stagingDir: string | undefined;
  let backupContainer: string | undefined;
  let backupPath: string | undefined;
  let backupSecured = false;
  let preserveBackupContainer = false;

  try {
    // mkdir is an exclusive lock acquisition. A contender leaves every path
    // belonging to the current installer untouched and fails clearly.
    try {
      await mkdir(lockPath, { mode: 0o700 });
    } catch (error) {
      rethrowLockError(error, lockPath);
    }
    lockAcquired = true;

    const proposedStagingDir = `${installParent}/.Keytap.app.stage-${randomUUID()}`;
    await mkdir(proposedStagingDir, { mode: 0o700 });
    stagingDir = proposedStagingDir;
    const stagedApp = `${stagingDir}/Keytap.app`;
    await Bun.$`ditto ${builtApp} ${stagedApp}`;

    const stagedBin = `${stagedApp}/Contents/MacOS/keytap`;
    const stagedInfo = await lstat(stagedBin);
    if (!stagedInfo.isFile() || (stagedInfo.mode & 0o111) === 0) {
      throw new Error("staged Keytap executable is missing or not executable");
    }
    await access(stagedBin, fsConstants.X_OK);

    // Inspect the live path only after owning the lock and validating the copy.
    let installedInfo;
    try {
      installedInfo = await lstat(installedApp);
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
    }

    if (installedInfo) {
      if (!installedInfo.isDirectory()) {
        throw new Error("installed Keytap path is not a directory; refusing replacement");
      }
      const acquiredBackupContainer = `${installParent}/.Keytap.install-${randomUUID()}`;
      await mkdir(acquiredBackupContainer, { mode: 0o700 });
      backupContainer = acquiredBackupContainer;
      backupPath = `${backupContainer}/Keytap.app`;
      await rename(installedApp, backupPath);
      backupSecured = true;
      try {
        await rename(stagedApp, installedApp);
      } catch (promotionError) {
        try {
          await rename(backupPath, installedApp);
          backupSecured = false;
        } catch (rollbackError) {
          console.error(
            `Keytap promotion and rollback failed; the previous app remains at ${backupPath}`,
            rollbackError,
          );
          preserveBackupContainer = true;
        }
        throw promotionError;
      }
      backupSecured = false;
      try {
        await rm(backupPath, { recursive: true });
      } catch (error) {
        preserveBackupContainer = true;
        console.error(
          `new Keytap app was installed, but backup cleanup failed; ${backupPath} may be partially removed and is not guaranteed to be a complete recovery copy`,
          error,
        );
      }
    } else {
      await rename(stagedApp, installedApp);
    }
  } catch (error) {
    if (backupSecured && backupPath) {
      console.error(`Keytap replacement failed; recovery copy remains at ${backupPath}`);
    } else {
      console.error("Keytap replacement failed; the installed app was not safely replaced");
    }
    throw error;
  } finally {
    if (stagingDir) {
      try {
        await rm(stagingDir, { recursive: true, force: true });
      } catch (error) {
        console.error(`could not remove this install's staging directory ${stagingDir}`, error);
      }
    }
    if (backupContainer && !preserveBackupContainer) {
      try {
        await rmdir(backupContainer);
      } catch (error) {
        console.error(`could not remove this install's backup container ${backupContainer}`, error);
      }
    }
    if (lockAcquired) {
      try {
        await rmdir(lockPath);
      } catch (error) {
        console.error(`could not release this install's lock ${lockPath}`, error);
        process.exitCode = 1;
      }
    }
  }
}

export async function install(
  args: readonly string[],
  home: string,
  debug: string,
  io: InstallIO,
) {
  const launchAgents = `${home}/Library/LaunchAgents`;
  const plistPath = `${launchAgents}/${LABEL}.plist`;
  const logPath = `${home}/Library/Logs/keytap.log`;

  let requestedBin: string | undefined;
  for (let i = 1; i < args.length; i++) {
    if (args[i] === "--bin") {
      const operand = args[++i];
      if (!operand) usage();
      requestedBin = resolve(operand);
    } else {
      usage();
    }
  }

  if (debug !== "0" && debug !== "1") {
    console.error(
      "KEYTAP_DEBUG_INSTALL must be exactly \"0\" or \"1\"",
    );
    process.exit(2);
  }

  const bin = requestedBin ?? defaultAppBin;
  validatePlistPath("binary path", bin);
  validatePlistPath("HOME-derived log path", logPath);

  const builtBin = `${builtApp}/Contents/MacOS/keytap`;
  if (requestedBin === undefined && !(await io.file(builtBin).exists())) {
    console.error(
      "Keytap.app not found. Build it first:\n  bun scripts/bundle.bun.ts",
    );
    process.exit(1);
  }

  // Use the account home so HOME overrides still share the launch agent lock.
  const lockPath = resolve(io.userInfo().homedir, "Library", `.${LABEL}.install-lock`);
  await io.mkdir(dirname(lockPath), { recursive: true });
  try {
    await io.mkdir(lockPath, { mode: 0o700 });
  } catch (error) {
    rethrowLockError(error, lockPath);
  }
  let lockReleaseFailed = false;
  try {
    if (requestedBin === undefined) await io.installBundle();
    if (!(await io.file(bin).exists())) {
      console.error(`binary not found: ${bin}`);
      process.exitCode = 1;
      return;
    }
    if (!bin.includes("Keytap.app/Contents/MacOS/")) {
      console.error(
        "warning: binary is not inside Keytap.app; Accessibility may attach to the wrong identity",
      );
    }

    const template = await io.file(templatePath).text();
    const values: Record<string, string> = {
      "@KEYTAP_BIN@": xmlEscape(bin),
      "@KEYTAP_LOG@": xmlEscape(logPath),
      "@KEYTAP_DEBUG@": xmlEscape(debug),
    };
    const rendered = template.replace(
      /@KEYTAP_(?:BIN|LOG|DEBUG)@/g,
      (placeholder) => values[placeholder],
    );

    await io.shell`mkdir -p ${launchAgents}`.quiet();
    await io.shell`mkdir -p ${home}/Library/Logs`.quiet();
    await io.write(plistPath, rendered);

    await io.shell`launchctl bootout gui/$UID/${LABEL}`.nothrow().quiet();
    const load = await io.shell`launchctl bootstrap gui/$UID ${plistPath}`.nothrow().quiet();
    if (load.exitCode !== 0) {
      await io.shell`launchctl unload ${plistPath}`.nothrow().quiet();
      const legacy = await io.shell`launchctl load ${plistPath}`.nothrow().quiet();
      if (legacy.exitCode !== 0) {
        console.error("launchctl load failed; plist written to", plistPath);
        process.exitCode = 1;
        return;
      }
    }
  } finally {
    try {
      await io.rmdir(lockPath);
    } catch (error) {
      console.error(`could not release this install's lock ${lockPath}`, error);
      process.exitCode = 1;
      lockReleaseFailed = true;
    }
  }
  if (lockReleaseFailed) return;
  console.log("installed", plistPath);
  console.log("binary  ", bin);
  console.log("log     ", logPath);
  console.log("Grant Accessibility to Keytap (the app), then:");
  console.log(`  open ${installedApp}`);
  console.log("or:");
  console.log(`  launchctl kickstart -k gui/$UID/${LABEL}`);
}

async function uninstall(home: string) {
  const plistPath = `${home}/Library/LaunchAgents/${LABEL}.plist`;
  await Bun.$`launchctl bootout gui/$UID/${LABEL}`.nothrow().quiet();
  await Bun.$`launchctl unload ${plistPath}`.nothrow().quiet();
  try {
    await Bun.$`rm -f ${plistPath}`;
  } catch {
    /* absent is fine */
  }
  try {
    await rm(installedApp, { recursive: true, force: true });
  } catch (error) {
    console.error(`could not remove ${installedApp}`, error);
    process.exitCode = 1;
  }
  console.log("uninstalled", LABEL);
}

if (import.meta.main) {
  const home = Bun.env.HOME;
  if (!home) {
    console.error("HOME is not set");
    process.exit(1);
  }

  const args = process.argv.slice(2);
  if (args[0] === "install") {
    try {
      await install(args, home, Bun.env.KEYTAP_DEBUG_INSTALL ?? "0", {
        userInfo,
        mkdir,
        rmdir,
        file: (path) => Bun.file(path),
        write: (path, content) => Bun.write(path, content),
        shell: (strings, ...values) => Bun.$(strings, ...values),
        installBundle,
      });
    } catch (error) {
      if (error instanceof LockHeldError) {
        console.error(error.message);
        process.exit(1);
      }
      throw error;
    }
  } else if (args[0] === "uninstall") {
    await uninstall(home);
  } else {
    usage();
  }
}

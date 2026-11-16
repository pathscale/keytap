import { expect, test } from "bun:test";
import { install, type InstallIO } from "./install.bun.ts";

const accountHome = "/fixture/account";
const firstHome = "/fixture/first";
const secondHome = "/fixture/second";
const accountLock = `${accountHome}/Library/.dev.keytap.install-lock`;
const firstPlist = `${firstHome}/Library/LaunchAgents/dev.keytap.plist`;
const secondPlist = `${secondHome}/Library/LaunchAgents/dev.keytap.plist`;
const bin = "/fixture/Keytap.app/Contents/MacOS/keytap";
const args = ["install", "--bin", bin];

function gate() {
  let open!: () => void;
  const promise = new Promise<void>((resolve) => {
    open = resolve;
  });
  return { promise, open };
}

function fixture({
  onBootstrap,
  writeFailure,
}: {
  onBootstrap?: (plist: string) => Promise<void>;
  writeFailure?: Error;
} = {}) {
  const directories = new Set([`${accountHome}/Library`]);
  const plists = new Map<string, string>();
  const effects: string[] = [];
  const io: InstallIO = {
    userInfo: () => ({ homedir: accountHome }),
    async mkdir(path, { recursive }) {
      if (directories.has(path)) {
        if (recursive) return;
        throw Object.assign(new Error(`already exists: ${path}`), { code: "EEXIST" });
      }
      effects.push(`mkdir ${path}`);
      directories.add(path);
    },
    async rmdir(path) {
      effects.push(`rmdir ${path}`);
      directories.delete(path);
    },
    file: (path) => ({
      exists: async () => path === bin,
      text: async () => "<plist>@KEYTAP_BIN@ @KEYTAP_LOG@ @KEYTAP_DEBUG@</plist>",
    }),
    async write(path, content) {
      effects.push(`write ${path}`);
      if (writeFailure) throw writeFailure;
      plists.set(path, content);
    },
    shell(strings, ...values) {
      const command = {
        nothrow: () => command,
        async quiet() {
          effects.push(`${strings[0]}${values.join(" ")}`);
          if (strings[0].startsWith("launchctl bootstrap ")) {
            await onBootstrap?.(values[0]);
          }
          return { exitCode: 0 };
        },
      };
      return command;
    },
    async installBundle() {
      effects.push("replace bundle");
      throw new Error("unexpected bundle replacement for --bin");
    },
  };
  return { io, directories, plists, effects };
}

test("concurrent installs with different HOME values share the account lock", async () => {
  const reachedBootstrap = gate();
  const finishBootstrap = gate();
  const { io, directories, plists, effects } = fixture({
    async onBootstrap(plist) {
      if (plist === firstPlist) {
        reachedBootstrap.open();
        await finishBootstrap.promise;
      }
    },
  });
  const winner = install(args, firstHome, "0", io);
  try {
    await Promise.race([
      reachedBootstrap.promise,
      winner.then(() => {
        throw new Error("install completed without bootstrapping");
      }),
    ]);
    expect(directories.has(accountLock)).toBe(true);
    expect(plists.has(firstPlist)).toBe(true);
    const beforeContender = effects.length;
    await expect(install(args, secondHome, "0", io)).rejects.toMatchObject({ code: "EEXIST" });
    expect(effects.slice(beforeContender)).toEqual([]);
    expect(plists.has(secondPlist)).toBe(false);
    expect(directories.has(accountLock)).toBe(true);
  } finally {
    finishBootstrap.open();
    await winner;
  }

  expect(directories.has(accountLock)).toBe(false);
  await install(args, secondHome, "0", io);
  expect(plists.get(secondPlist)).toContain(`${secondHome}/Library/Logs/keytap.log`);
  expect(directories.has(accountLock)).toBe(false);
});

test("a failed plist write releases the account lock", async () => {
  const writeFailure = new Error("synthetic plist write failure");
  const { io, directories } = fixture({ writeFailure });
  await expect(install(args, firstHome, "0", io)).rejects.toBe(writeFailure);
  expect(directories.has(accountLock)).toBe(false);
});

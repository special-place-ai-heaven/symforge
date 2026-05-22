#!/usr/bin/env node
"use strict";

const childProcess = require("child_process");
const fs = require("fs");
const path = require("path");
const os = require("os");

function createLauncher(overrides = {}) {
  const fsMod = overrides.fs || fs;
  const pathMod = overrides.path || path;
  const osMod = overrides.os || os;
  const processMod = overrides.process || process;
  const consoleMod = overrides.console || console;
  const spawnSyncFn = overrides.spawnSync || childProcess.spawnSync;
  const execFileSyncFn = overrides.execFileSync || childProcess.execFileSync;
  const packageJson = overrides.packageJson || require("../package.json");
  const installScriptPath = overrides.installScriptPath
    || pathMod.join(__dirname, "..", "scripts", "install.js");

  function resolveInstallDir() {
    if (overrides.installDir) {
      return overrides.installDir;
    }
    if (processMod.env.SYMFORGE_HOME) {
      return pathMod.join(processMod.env.SYMFORGE_HOME, "bin");
    }
    return pathMod.join(osMod.homedir(), ".symforge", "bin");
  }

  const ext = processMod.platform === "win32" ? ".exe" : "";
  const installDir = resolveInstallDir();
  const binPath = pathMod.join(installDir, "symforge" + ext);
  const cmdShimPath = pathMod.join(installDir, "symforge.cmd");
  const pendingPath = pathMod.join(installDir, "symforge.pending" + ext);
  const versionPath = pathMod.join(installDir, "symforge.version");
  const pendingVersionPath = pathMod.join(installDir, "symforge.pending.version");

  function getLaunchPath() {
    if (
      processMod.platform === "win32"
      && !fsMod.existsSync(binPath)
      && fsMod.existsSync(cmdShimPath)
    ) {
      return cmdShimPath;
    }
    return binPath;
  }

  function isWindowsCmdShim(targetPath) {
    return (
      processMod.platform === "win32"
      && pathMod.extname(targetPath).toLowerCase() === ".cmd"
    );
  }

  function childOptionsFor(targetPath, baseOptions) {
    const options = { ...baseOptions, env: processMod.env };
    if (isWindowsCmdShim(targetPath)) {
      options.shell = true;
    }
    return options;
  }

  function relayInstallerOutput(output) {
    if (!output) {
      return;
    }
    const text = typeof output === "string" ? output : String(output);
    for (const line of text.split(/\r?\n/)) {
      if (line) {
        consoleMod.error(line);
      }
    }
  }

  function parseVersion(text) {
    if (!text) {
      return null;
    }
    const match = String(text).match(/(\d+\.\d+\.\d+)/);
    return match ? match[1] : null;
  }

  function readRecordedVersion(targetPath) {
    try {
      return parseVersion(fsMod.readFileSync(targetPath, "utf8").trim());
    } catch {
      return null;
    }
  }

  function writeRecordedVersion(targetPath, version) {
    if (!version) {
      return;
    }
    try {
      fsMod.writeFileSync(targetPath, `${version}\n`);
    } catch {
      // Best-effort metadata only.
    }
  }

  function getInstalledVersion() {
    const recordedVersion = readRecordedVersion(versionPath);
    if (recordedVersion) {
      return recordedVersion;
    }

    try {
      const launchPath = getLaunchPath();
      const output = execFileSyncFn(
        launchPath,
        ["--version"],
        childOptionsFor(launchPath, {
          encoding: "utf8",
          timeout: 5000,
        }),
      ).trim();
      const parsedVersion = parseVersion(output);
      writeRecordedVersion(versionPath, parsedVersion);
      return parsedVersion;
    } catch {
      return null;
    }
  }

  function applyPendingUpdate() {
    if (!fsMod.existsSync(pendingPath)) {
      return false;
    }

    try {
      fsMod.renameSync(pendingPath, binPath);
      if (fsMod.existsSync(pendingVersionPath)) {
        fsMod.renameSync(pendingVersionPath, versionPath);
      } else {
        writeRecordedVersion(versionPath, packageJson.version);
      }
      consoleMod.error("symforge: applied pending update.");
      return true;
    } catch {
      return false;
    }
  }

  function runInstaller() {
    try {
      const stdout = execFileSyncFn(processMod.execPath, [installScriptPath], {
        encoding: "utf8",
        stdio: ["ignore", "pipe", "pipe"],
        env: processMod.env,
      });
      relayInstallerOutput(stdout);
    } catch (error) {
      relayInstallerOutput(error.stdout);
      relayInstallerOutput(error.stderr);
      throw error;
    }
  }

  function detectClients() {
    const clients = [];
    const home = osMod.homedir();
    if (fsMod.existsSync(pathMod.join(home, ".claude"))) clients.push("claude");
    if (fsMod.existsSync(pathMod.join(home, ".codex"))) clients.push("codex");
    if (fsMod.existsSync(pathMod.join(home, ".gemini"))) clients.push("gemini");
    if (clients.length === 0) return null;
    if (clients.length >= 2) return "all";
    return clients[0];
  }

  function runAutoInit() {
    const client = detectClients();
    if (client === null) {
      consoleMod.error(
        "symforge: auto-init skipped — no supported clients detected."
      );
      return;
    }
    consoleMod.error(`symforge: auto-configuring for ${client}...`);
    try {
      const launchPath = getLaunchPath();
      const output = execFileSyncFn(
        launchPath,
        ["init", "--client", client],
        childOptionsFor(launchPath, {
          encoding: "utf8",
          timeout: 15000,
        }),
      );
      relayInstallerOutput(output);
    } catch (error) {
      consoleMod.error(
        `symforge: auto-init warning: ${error.message}`
      );
    }
  }

  function ensureInstalledBinary() {
    const pendingApplied = applyPendingUpdate();

    const expectedVersion = packageJson.version;
    const hasBinary = fsMod.existsSync(getLaunchPath());
    const installedVersion = hasBinary ? getInstalledVersion() : null;

    if (installedVersion === expectedVersion) {
      // If a pending update was just applied, run init to ensure config matches
      if (pendingApplied) {
        runAutoInit();
      }
      return;
    }

    if (!hasBinary) {
      consoleMod.error("symforge binary not found. Running install...");
    } else {
      consoleMod.error(
        `symforge binary version ${installedVersion || "unknown"} does not match wrapper version ${expectedVersion}. Running install...`
      );
    }

    runInstaller();
    applyPendingUpdate();

    if (!fsMod.existsSync(getLaunchPath())) {
      throw new Error("symforge binary is still missing after install.");
    }
  }

  function main(args) {
    ensureInstalledBinary();
    const launchPath = getLaunchPath();
    const result = spawnSyncFn(
      launchPath,
      args,
      childOptionsFor(launchPath, {
        stdio: "inherit",
      }),
    );
    return result.status ?? 1;
  }

  return {
    applyPendingUpdate,
    detectClients,
    ensureInstalledBinary,
    getInstalledVersion,
    getBinaryPath: () => getLaunchPath(),
    getPendingPath: () => pendingPath,
    main,
    runAutoInit,
  };
}

module.exports = { createLauncher };

if (require.main === module) {
  process.exit(createLauncher().main(process.argv.slice(2)));
}

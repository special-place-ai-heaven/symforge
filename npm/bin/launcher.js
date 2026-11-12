#!/usr/bin/env node
"use strict";

const childProcess = require("child_process");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { resolveBinary, formatResolveError } = require("../lib/resolve-binary.js");

// Best-effort, path-only scan of $PATH for every `symforge` entry, in PATH
// order. Never spawns anything (this runs on every MCP server start, so it must
// add negligible latency). On Windows it honors PATHEXT (defaulting to the
// usual shim extensions) so the npm `.cmd`/`.exe` shim is found. Returns the
// list of existing candidate paths, deduplicated, in the order PATH would
// resolve them.
function whichAllSymforge(env, platform) {
  const environ = env || {};
  const rawPath = environ.PATH || environ.Path || "";
  if (!rawPath) return [];

  const isWindows = platform === "win32";
  // Candidate basenames to probe inside each PATH dir. On Windows, PATHEXT
  // decides which extensions are executable; we only care about the ones a
  // symforge shim would ever use. The bare name covers a shell function / shim
  // without an extension and the WSL-bleed case where a Linux `symforge` sits
  // on PATH.
  let names = ["symforge"];
  if (isWindows) {
    const pathext = (environ.PATHEXT || ".COM;.EXE;.BAT;.CMD")
      .split(";")
      .map((ext) => ext.trim().toLowerCase())
      .filter((ext) => ext === ".exe" || ext === ".cmd" || ext === ".bat" || ext === ".com");
    const exts = pathext.length > 0 ? pathext : [".exe", ".cmd"];
    names = ["symforge", ...exts.map((ext) => `symforge${ext}`)];
  }

  const found = [];
  const seen = new Set();
  for (const dir of rawPath.split(path.delimiter)) {
    if (!dir) continue;
    for (const name of names) {
      const candidate = path.join(dir, name);
      if (seen.has(candidate)) continue;
      seen.add(candidate);
      try {
        const stat = fs.statSync(candidate);
        if (stat.isFile()) found.push(candidate);
      } catch (_err) {
        // Missing path entry — skip silently.
      }
    }
  }
  return found;
}

// Same file on disk: realpaths equal, so a symlinked npm bin shim
// (`<prefix>/bin/symforge` -> `.../symforge/bin/symforge.js`) matches its target.
// A path that cannot be resolved falls back to its absolute form.
function sameFile(a, b) {
  const real = (p) => {
    try {
      return fs.realpathSync.native(p);
    } catch (_err) {
      return path.resolve(p);
    }
  };
  return real(a) === real(b);
}

// True when `shadowPath` is THIS launcher's own npm bin shim, not a foreign
// install. The thing on PATH is normally the npm-generated shim
// (`symforge` / `symforge.cmd`), which lives in the npm prefix `bin` dir that
// is a sibling of this package's install dir (…/node_modules/symforge). We
// treat a shim as "ours" when it sits in the bin dir adjacent to our package
// root, or when its file contents reference this package's launcher. Either
// proves the first PATH `symforge` would dispatch back into this very launcher,
// so there is nothing to warn about.
function isOwnShim(shadowPath, selfDir) {
  try {
    // selfDir is …/symforge/bin (this launcher's directory). The package root
    // is its parent; the install prefix bin dir is a sibling of node_modules.
    const packageRoot = path.dirname(selfDir); // …/symforge
    const nodeModules = path.dirname(packageRoot); // …/node_modules
    const prefix = path.dirname(nodeModules); // npm prefix root
    const shimDir = path.dirname(path.resolve(shadowPath));

    // npm places global shims in <prefix>/bin (POSIX) or <prefix> (Windows).
    const prefixBinPosix = path.join(prefix, "bin");
    if (shimDir === prefix || shimDir === prefixBinPosix) {
      return true;
    }

    // The shim may itself be the package's own bin file (e.g. local linked
    // install where PATH points directly at bin/symforge.js's dir).
    if (shimDir === selfDir) {
      return true;
    }

    // Fall back to content inspection: npm shims embed the relative path back
    // to the package launcher. If the shim text references this package, it is
    // ours regardless of where the prefix sits.
    const text = fs.readFileSync(path.resolve(shadowPath), "utf8");
    if (text.includes("node_modules/symforge/bin/") || text.includes("node_modules\\symforge\\bin\\")) {
      return true;
    }
  } catch (_err) {
    // Unreadable / non-shim — fall through and treat as foreign so we do not
    // suppress a genuine shadow on a transient read error.
  }
  return false;
}

// Classify the shadowing install the same way the native binary does, so the
// one-liner matches operator mental models from `symforge health` / `init`.
function classifyShadow(shadowPath, env, platform) {
  const environ = env || {};
  if (platform !== "win32") {
    let procVersion = "";
    try {
      procVersion = fs.readFileSync("/proc/version", "utf8").toLowerCase();
    } catch (_err) {
      // Not Linux / unreadable — not WSL.
    }
    const underWsl = /microsoft|wsl/.test(procVersion);
    if (underWsl && /^\/mnt\/[a-z]\//i.test(shadowPath)) {
      return "windows-bleed";
    }
    if (/^\/usr\/local\//.test(shadowPath) || /^\/usr\//.test(shadowPath) || /^\/opt\//.test(shadowPath)) {
      return "root/system";
    }
  }
  return "foreign-prefix";
}

// Best-effort PATH-shadow warning. When the launcher runs but a DIFFERENT
// `symforge` would win on $PATH, print one or two stderr lines pointing the
// operator at the binary's richer remediation. NEVER blocks or breaks the
// spawn: the entire body is wrapped in try/catch by the caller and any failure
// is swallowed. Output goes to stderr only (stdout is the MCP stdio channel).
function maybeWarnPathShadow(consoleMod, env, platform, resolvedBinaryPath, selfDir) {
  const environ = env || {};
  if (environ.SYMFORGE_NO_SHADOW_WARN) return;

  const entries = whichAllSymforge(environ, platform);
  if (entries.length === 0) return;

  const first = entries[0];

  // The launcher already "wins" when the first PATH symforge IS the resolved
  // native binary, IS this launcher itself (by realpath, so a symlinked bin
  // shim in any prefix counts), or is this launcher's own npm shim that
  // dispatches here.
  if (sameFile(first, resolvedBinaryPath)) return;
  if (sameFile(first, path.join(selfDir, "symforge.js"))) return;
  if (sameFile(first, path.join(selfDir, "launcher.js"))) return;
  if (isOwnShim(first, selfDir)) return;

  const kind = classifyShadow(first, environ, platform);
  consoleMod.error(
    `symforge: warning: a different 'symforge' (${kind}) is first on your PATH: ${first}`
  );
  consoleMod.error(
    "  This launcher is running, but that one shadows it. Run 'symforge init' (or see 'symforge health') for exact fix commands."
  );
}

// Signals a caller (an agent harness, a container stop) sends to end the server.
const FORWARDED_SIGNALS = Object.freeze(["SIGTERM", "SIGINT", "SIGHUP"]);

// Run the native binary and settle once it has EXITED, never before (#877).
// `spawnSync` blocked the event loop, so no signal handler could run: a SIGTERM
// killed this launcher and orphaned the native child, which then exited
// unreaped and, under a PID 1 that is not an init, stayed a zombie forever.
// Forwarding the signal and waiting for `exit` means this process always
// collects its child. A SIGKILL to the launcher cannot be forwarded; that gap
// stays. Windows has no zombies and its "signals" (Ctrl+C) already reach the
// child through the shared console, so nothing is forwarded there.
function runChildForwardingSignals(command, args, options, spawnFn = childProcess.spawn) {
  return new Promise((resolve) => {
    // Handlers go in BEFORE the spawn: a signal landing between spawn() and
    // the handler install would otherwise still kill this launcher alone. One
    // that arrives before the child exists is held and delivered right after.
    let child;
    let early;
    const signals = process.platform === "win32" ? [] : FORWARDED_SIGNALS;
    const forward = (signal) => {
      if (child) child.kill(signal);
      else early = signal;
    };
    const settle = (result) => {
      for (const signal of signals) process.removeListener(signal, forward);
      resolve(result);
    };
    for (const signal of signals) process.on(signal, forward);
    try {
      child = spawnFn(command, args, options);
    } catch (error) {
      // Synchronous spawn failures (EINVAL, E2BIG, EPERM, ...) take the same
      // path as the 'error' event so exit codes match the old spawnSync ones.
      settle({ error });
      return;
    }
    if (early) child.kill(early);
    // 'error' is terminal only when the spawn itself failed (no pid); a failed
    // kill also emits 'error', and the child is still ours to wait for then.
    child.on("error", (error) => {
      if (child.pid === undefined) settle({ error });
    });
    child.once("exit", (status, signal) => settle({ status, signal }));
  });
}

function createLauncher(overrides = {}) {
  const processMod = overrides.process || process;
  const consoleMod = overrides.console || console;
  const runChildFn = overrides.runChild || runChildForwardingSignals;
  const resolveBinaryFn = overrides.resolveBinary || ((opts) => resolveBinary({
    ...opts,
    platform: processMod.platform,
    arch: processMod.arch,
    requireResolve: overrides.requireResolve || require.resolve,
  }));
  const selfDir = overrides.selfDir || __dirname;

  async function main(args) {
    const resolved = resolveBinaryFn({ binary: "symforge" });
    if (resolved.reason !== "ok") {
      consoleMod.error(formatResolveError(resolved, {
        platform: processMod.platform,
        arch: processMod.arch,
      }));
      return 64;
    }

    // Best-effort PATH-shadow warning. Must never block or break the spawn, so
    // swallow any error from the detection path entirely.
    try {
      maybeWarnPathShadow(
        consoleMod,
        processMod.env,
        processMod.platform,
        resolved.binaryPath,
        selfDir
      );
    } catch (_err) {
      // Detection failure must not stop symforge from running.
    }

    const result = await runChildFn(resolved.binaryPath, args, {
      stdio: "inherit",
      env: processMod.env,
      shell: false,
      // Never a visible console window, even from a console-less parent. It
      // only sets SW_HIDE on Windows; stdio inheritance is unaffected.
      windowsHide: true,
    });
    if (result.error) {
      consoleMod.error(`symforge: failed to spawn ${resolved.binaryPath}: ${result.error.code || result.error.message}`);
      return result.error.code === "ENOENT" ? 127 : 126;
    }
    if (result.status !== null && result.status !== undefined) return result.status;
    // Killed by a signal: report it the shell way (128 + signal number).
    const signalNumber = result.signal ? os.constants.signals[result.signal] : undefined;
    return signalNumber ? 128 + signalNumber : 1;
  }

  return {
    main,
  };
}

module.exports = {
  createLauncher,
  runChildForwardingSignals,
  whichAllSymforge,
  isOwnShim,
  classifyShadow,
};

if (require.main === module) {
  createLauncher().main(process.argv.slice(2)).then(
    (code) => process.exit(code),
    (err) => {
      console.error(err.message);
      process.exit(1);
    }
  );
}

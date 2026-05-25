#!/usr/bin/env node
"use strict";

const childProcess = require("child_process");
const { resolveBinary, formatResolveError } = require("../lib/resolve-binary.js");

function createLauncher(overrides = {}) {
  const processMod = overrides.process || process;
  const consoleMod = overrides.console || console;
  const spawnSyncFn = overrides.spawnSync || childProcess.spawnSync;
  const resolveBinaryFn = overrides.resolveBinary || ((opts) => resolveBinary({
    ...opts,
    platform: processMod.platform,
    arch: processMod.arch,
    requireResolve: overrides.requireResolve || require.resolve,
  }));

  function main(args) {
    const resolved = resolveBinaryFn({ binary: "symforge" });
    if (resolved.reason !== "ok") {
      consoleMod.error(formatResolveError(resolved, {
        platform: processMod.platform,
        arch: processMod.arch,
      }));
      return 64;
    }

    const result = spawnSyncFn(resolved.binaryPath, args, {
      stdio: "inherit",
      env: processMod.env,
      shell: false,
    });
    if (result.error) {
      consoleMod.error(`symforge: failed to spawn ${resolved.binaryPath}: ${result.error.code || result.error.message}`);
      return result.error.code === "ENOENT" ? 127 : 126;
    }
    return result.status ?? 1;
  }

  return {
    main,
  };
}

module.exports = { createLauncher };

if (require.main === module) {
  process.exit(createLauncher().main(process.argv.slice(2)));
}

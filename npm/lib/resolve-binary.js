"use strict";

const fs = require("fs");
const path = require("path");

const SUPPORTED_TARGETS = Object.freeze([
  Object.freeze({
    platform: "linux",
    arch: "x64",
    pkg: "symforge-linux-x64",
    monorepoDir: "linux-x64",
    binaryName: "symforge",
  }),
  Object.freeze({
    platform: "darwin",
    arch: "arm64",
    pkg: "symforge-macos-arm64",
    monorepoDir: "macos-arm64",
    binaryName: "symforge",
  }),
  Object.freeze({
    platform: "darwin",
    arch: "x64",
    pkg: "symforge-macos-x64",
    monorepoDir: "macos-x64",
    binaryName: "symforge",
  }),
  Object.freeze({
    platform: "win32",
    arch: "x64",
    pkg: "symforge-windows-x64",
    monorepoDir: "windows-x64",
    binaryName: "symforge.exe",
  }),
]);

const ALLOWED_BINARIES = Object.freeze(["symforge"]);

function findPlatformPackageJson(target, requireResolve) {
  try {
    return requireResolve(`${target.pkg}/package.json`);
  } catch (_err) {
    // Fall through to local development lookup.
  }

  const local = path.join(__dirname, "..", "platforms", target.monorepoDir, "package.json");
  if (fs.existsSync(local)) {
    return local;
  }

  let dir = path.join(__dirname, "..");
  for (let i = 0; i < 6; i += 1) {
    const nested = path.join(dir, "node_modules", target.pkg, "package.json");
    if (fs.existsSync(nested)) {
      return nested;
    }
    const parent = path.dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }

  return null;
}

function resolveBinary(opts) {
  const options = opts || {};
  const binary = options.binary;
  const platform = options.platform || process.platform;
  const arch = options.arch || process.arch;
  const requireResolve = options.requireResolve || require.resolve;

  if (!ALLOWED_BINARIES.includes(binary)) {
    return {
      reason: "invalid_binary",
      platformPackage: null,
      binaryPath: null,
      supportedTargets: SUPPORTED_TARGETS,
    };
  }

  const target = SUPPORTED_TARGETS.find((t) => t.platform === platform && t.arch === arch);
  if (!target) {
    return {
      reason: "unsupported_platform",
      platformPackage: null,
      binaryPath: null,
      supportedTargets: SUPPORTED_TARGETS,
    };
  }

  const pkgJsonPath = findPlatformPackageJson(target, requireResolve);
  if (!pkgJsonPath) {
    return {
      reason: "platform_package_missing",
      platformPackage: target.pkg,
      binaryPath: null,
      supportedTargets: SUPPORTED_TARGETS,
    };
  }

  const binaryPath = path.join(path.dirname(pkgJsonPath), "bin", target.binaryName);
  if (!fs.existsSync(binaryPath)) {
    return {
      reason: "platform_package_missing",
      platformPackage: target.pkg,
      binaryPath: null,
      supportedTargets: SUPPORTED_TARGETS,
    };
  }

  return {
    reason: "ok",
    platformPackage: target.pkg,
    binaryPath,
    supportedTargets: SUPPORTED_TARGETS,
  };
}

function formatResolveError(result, opts) {
  const options = opts || {};
  const platform = options.platform || process.platform;
  const arch = options.arch || process.arch;
  const targets = result.supportedTargets.map((t) => `${t.platform}-${t.arch}`).join(", ");

  if (result.reason === "ok") return null;
  if (result.reason === "unsupported_platform") {
    return `symforge: unsupported platform ${platform}-${arch}; supported: ${targets}`;
  }
  if (result.reason === "platform_package_missing") {
    return (
      `symforge: platform package ${result.platformPackage} not installed or missing its binary. ` +
      "Reinstall with optional dependencies enabled: npm install -g symforge@latest"
    );
  }
  if (result.reason === "invalid_binary") {
    return "symforge: internal error: invalid binary name passed to resolver";
  }
  return `symforge: unknown resolver state ${result.reason}`;
}

module.exports = {
  ALLOWED_BINARIES,
  SUPPORTED_TARGETS,
  formatResolveError,
  resolveBinary,
};

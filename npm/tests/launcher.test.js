const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const { createLauncher, runChildForwardingSignals } = require("../bin/launcher.js");
const {
  ALLOWED_BINARIES,
  SUPPORTED_TARGETS,
  formatResolveError,
  resolveBinary,
} = require("../lib/resolve-binary.js");

function withPlatformPackage(t, packageName, binaryName) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "symforge-platform-"));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));

  const pkgRoot = path.join(root, packageName);
  const binDir = path.join(pkgRoot, "bin");
  fs.mkdirSync(binDir, { recursive: true });
  fs.writeFileSync(path.join(pkgRoot, "package.json"), JSON.stringify({ name: packageName }));
  fs.writeFileSync(path.join(binDir, binaryName), "binary");

  return {
    root,
    packageJsonPath: path.join(pkgRoot, "package.json"),
    binaryPath: path.join(binDir, binaryName),
  };
}

test("resolveBinary maps win32-x64 to the packaged Windows executable", (t) => {
  const fixture = withPlatformPackage(t, "symforge-windows-x64", "symforge.exe");
  const result = resolveBinary({
    binary: "symforge",
    platform: "win32",
    arch: "x64",
    requireResolve(specifier) {
      assert.equal(specifier, "symforge-windows-x64/package.json");
      return fixture.packageJsonPath;
    },
  });

  assert.equal(result.reason, "ok");
  assert.equal(result.platformPackage, "symforge-windows-x64");
  assert.equal(result.binaryPath, fixture.binaryPath);
});

test("resolveBinary rejects unsupported platforms without shelling out", () => {
  const result = resolveBinary({
    binary: "symforge",
    platform: "freebsd",
    arch: "x64",
    requireResolve() {
      throw new Error("requireResolve must not be called");
    },
  });

  assert.equal(result.reason, "unsupported_platform");
  assert.match(formatResolveError(result, { platform: "freebsd", arch: "x64" }), /unsupported platform freebsd-x64/);
});

test("resolveBinary reports missing optional platform package", () => {
  const result = resolveBinary({
    binary: "symforge",
    platform: "linux",
    arch: "x64",
    requireResolve() {
      throw new Error("not installed");
    },
  });

  assert.equal(result.reason, "platform_package_missing");
  assert.equal(result.platformPackage, "symforge-linux-x64");
});

test("formatResolveError diagnoses the WSL Windows-prefix trap for a /mnt launcher", () => {
  const result = resolveBinary({
    binary: "symforge",
    platform: "linux",
    arch: "x64",
    requireResolve() {
      throw new Error("not installed");
    },
  });

  const message = formatResolveError(result, {
    platform: "linux",
    arch: "x64",
    selfPath: "/mnt/c/Users/rakovnik/.npm-global/node_modules/symforge/lib",
  });

  assert.match(message, /Windows npm install running under linux/);
  assert.match(message, /\/mnt\/c\/Users\/rakovnik/);
  assert.match(message, /shared Windows ~\/\.npmrc 'prefix='/);
  assert.match(message, /npm config set prefix "\$HOME\/\.npm-global"/);
  assert.match(message, /symforge init --client all/);
});

test("formatResolveError keeps the generic missing-package message off Windows mounts", () => {
  const result = resolveBinary({
    binary: "symforge",
    platform: "linux",
    arch: "x64",
    requireResolve() {
      throw new Error("not installed");
    },
  });

  const message = formatResolveError(result, {
    platform: "linux",
    arch: "x64",
    selfPath: "/home/rakovnik/.npm-global/lib/node_modules/symforge/lib",
  });

  assert.match(message, /not installed or missing its binary/);
  assert.doesNotMatch(message, /Windows drive mount/);
});

test("formatResolveError never shows the WSL hint to a real Windows host", () => {
  const result = resolveBinary({
    binary: "symforge",
    platform: "win32",
    arch: "x64",
    requireResolve() {
      throw new Error("not installed");
    },
  });

  const message = formatResolveError(result, {
    platform: "win32",
    arch: "x64",
    selfPath: "/mnt/c/whatever",
  });

  assert.match(message, /not installed or missing its binary/);
  assert.doesNotMatch(message, /Windows drive mount/);
});

test("resolveBinary rejects invalid binary names", () => {
  const result = resolveBinary({ binary: "other", platform: "linux", arch: "x64" });

  assert.equal(result.reason, "invalid_binary");
  assert.deepEqual(ALLOWED_BINARIES, ["symforge"]);
  assert.ok(SUPPORTED_TARGETS.some((target) => target.platform === "win32" && target.arch === "x64"));
});

test("launcher spawns the resolved native binary directly", async () => {
  const spawnCalls = [];
  const errors = [];
  const launcher = createLauncher({
    process: { platform: "win32", arch: "x64", env: { A: "B" } },
    console: { error: (message) => errors.push(message) },
    resolveBinary() {
      return {
        reason: "ok",
        binaryPath: "C:\\\\tools\\\\symforge\\\\symforge.exe",
      };
    },
    runChild(command, args, options) {
      spawnCalls.push({ command, args, options });
      return { status: 0 };
    },
  });

  assert.equal(await launcher.main(["--version"]), 0);
  assert.deepEqual(spawnCalls, [{
    command: "C:\\\\tools\\\\symforge\\\\symforge.exe",
    args: ["--version"],
    options: { stdio: "inherit", env: { A: "B" }, shell: false, windowsHide: true },
  }]);
  assert.deepEqual(errors, []);
});

test("launcher returns 64 when the platform package is unavailable", async () => {
  const errors = [];
  const launcher = createLauncher({
    process: { platform: "linux", arch: "arm64", env: {} },
    console: { error: (message) => errors.push(message) },
    runChild() {
      throw new Error("runChild must not be called");
    },
  });

  assert.equal(await launcher.main([]), 64);
  assert.match(errors.join("\n"), /unsupported platform linux-arm64/);
});

test("launcher maps spawn failures to process-style error codes", async () => {
  const errors = [];
  const launcher = createLauncher({
    process: { platform: "linux", arch: "x64", env: {} },
    console: { error: (message) => errors.push(message) },
    resolveBinary() {
      return { reason: "ok", binaryPath: "/usr/local/bin/symforge" };
    },
    runChild() {
      return { error: Object.assign(new Error("permission denied"), { code: "EACCES" }) };
    },
  });

  assert.equal(await launcher.main([]), 126);
  assert.match(errors.join("\n"), /failed to spawn/);
});

// #877: a SIGTERM to the launcher must reach the native binary and the
// launcher must wait for it. Before the fix, `spawnSync` let the signal kill
// the launcher alone; the orphaned native process exited later unreaped and,
// under a PID 1 that is not an init, stayed a zombie forever.
test("launcher forwards SIGTERM to the native binary and exits only after it", { skip: process.platform === "win32" }, async (t) => {
  const { spawn } = require("node:child_process");
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "symforge-signal-"));
  const pidFile = path.join(dir, "native.pid");
  const fakeBinary = path.join(dir, "symforge");
  fs.writeFileSync(fakeBinary, `#!/bin/sh\necho $$ > '${pidFile}'\nexec sleep 300\n`, { mode: 0o755 });
  let nativePid;
  let launcher;
  // Registered first so a failing assert below leaves no process behind.
  t.after(() => {
    for (const pid of [launcher && launcher.pid, nativePid]) {
      if (!pid) continue;
      try { process.kill(pid, "SIGKILL"); } catch (_err) { /* already gone */ }
    }
    fs.rmSync(dir, { recursive: true, force: true });
  });

  const launcherPath = path.join(__dirname, "..", "bin", "launcher.js");
  const script = `require(${JSON.stringify(launcherPath)}).createLauncher({`
    + `resolveBinary: () => ({ reason: "ok", binaryPath: ${JSON.stringify(fakeBinary)} }),`
    + `}).main([]).then((code) => process.exit(code));`;
  launcher = spawn(process.execPath, ["-e", script], {
    stdio: "ignore",
    env: { ...process.env, SYMFORGE_NO_SHADOW_WARN: "1" },
  });
  const exited = new Promise((resolve) => launcher.once("exit", (code, signal) => resolve({ code, signal })));

  const deadline = Date.now() + 10000;
  const readPid = () => (fs.existsSync(pidFile) ? Number(fs.readFileSync(pidFile, "utf8").trim()) : 0);
  while (!(nativePid = readPid())) {
    assert.ok(Date.now() < deadline, "the fake native binary never started within 10s");
    await new Promise((resolve) => setTimeout(resolve, 20));
  }

  launcher.kill("SIGTERM");
  const { code, signal } = await exited;

  assert.throws(() => process.kill(nativePid, 0), { code: "ESRCH" },
    "the native binary outlived the launcher: the SIGTERM was not forwarded or not waited for");
  assert.deepEqual({ code, signal }, { code: 128 + os.constants.signals.SIGTERM, signal: null });
});

test("a synchronous spawn failure maps to the same exit code as before", async () => {
  const einval = Object.assign(new Error("invalid argument"), { code: "EINVAL" });
  const result = await runChildForwardingSignals("/x/symforge", [], {}, () => { throw einval; });
  assert.deepEqual(result, { error: einval });

  const errors = [];
  const launcher = createLauncher({
    process: { platform: "linux", arch: "x64", env: {} },
    console: { error: (message) => errors.push(message) },
    resolveBinary: () => ({ reason: "ok", binaryPath: "/x/symforge" }),
    runChild: (command, args, options) => runChildForwardingSignals(command, args, options, () => { throw einval; }),
  });
  assert.equal(await launcher.main([]), 126);
  assert.match(errors.join("\n"), /EINVAL/);
});

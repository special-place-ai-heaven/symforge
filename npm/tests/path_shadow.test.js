const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const {
  createLauncher,
  whichAllSymforge,
  isOwnShim,
  classifyShadow,
} = require("../bin/launcher.js");

// Create a temp dir that auto-cleans after the test, and return its path.
function tmpDir(t, prefix) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), prefix));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  return root;
}

// Write a fake `symforge` (or named) file inside `dir` and return its path.
function writeFakeBinary(dir, name = "symforge", contents = "fake") {
  fs.mkdirSync(dir, { recursive: true });
  const file = path.join(dir, name);
  fs.writeFileSync(file, contents);
  return file;
}

test("whichAllSymforge returns existing PATH entries in PATH order", (t) => {
  const root = tmpDir(t, "symforge-which-");
  const dirA = path.join(root, "a");
  const dirB = path.join(root, "b");
  const dirEmpty = path.join(root, "empty");
  fs.mkdirSync(dirEmpty, { recursive: true });

  const binA = writeFakeBinary(dirA);
  const binB = writeFakeBinary(dirB);

  const env = { PATH: [dirA, dirEmpty, dirB].join(path.delimiter) };
  const found = whichAllSymforge(env, "linux");

  assert.deepEqual(found, [binA, binB]);
});

test("whichAllSymforge skips directories with no symforge", (t) => {
  const root = tmpDir(t, "symforge-which-skip-");
  const dirA = path.join(root, "a");
  const dirB = path.join(root, "b");
  fs.mkdirSync(dirA, { recursive: true });
  const binB = writeFakeBinary(dirB);

  const env = { PATH: [dirA, dirB].join(path.delimiter) };
  const found = whichAllSymforge(env, "linux");

  assert.deepEqual(found, [binB]);
});

test("whichAllSymforge ignores a directory that shares the symforge name", (t) => {
  const root = tmpDir(t, "symforge-which-dir-");
  const dir = path.join(root, "a");
  // A *directory* literally named symforge must not count as an executable.
  fs.mkdirSync(path.join(dir, "symforge"), { recursive: true });

  const env = { PATH: dir };
  const found = whichAllSymforge(env, "linux");

  assert.deepEqual(found, []);
});

test("whichAllSymforge returns [] when PATH is unset", () => {
  assert.deepEqual(whichAllSymforge({}, "linux"), []);
  assert.deepEqual(whichAllSymforge({ PATH: "" }, "linux"), []);
});

test("whichAllSymforge resolves Windows extensions via PATHEXT", (t) => {
  const root = tmpDir(t, "symforge-which-win-");
  const dir = path.join(root, "bin");
  const cmd = writeFakeBinary(dir, "symforge.cmd");

  const env = {
    Path: dir,
    PATHEXT: ".COM;.EXE;.BAT;.CMD",
  };
  const found = whichAllSymforge(env, "win32");

  assert.deepEqual(found, [cmd]);
});

test("whichAllSymforge prefers the bare name before the extensioned shim within a dir", (t) => {
  const root = tmpDir(t, "symforge-which-order-");
  const dir = path.join(root, "bin");
  const bare = writeFakeBinary(dir, "symforge");
  const exe = writeFakeBinary(dir, "symforge.exe");

  const env = { Path: dir, PATHEXT: ".EXE" };
  const found = whichAllSymforge(env, "win32");

  // Both exist; the bare candidate is probed first within the directory.
  assert.deepEqual(found, [bare, exe]);
});

test("classifyShadow flags /usr/local as root/system", () => {
  assert.equal(classifyShadow("/usr/local/bin/symforge", {}, "linux"), "root/system");
  assert.equal(classifyShadow("/usr/bin/symforge", {}, "linux"), "root/system");
  assert.equal(classifyShadow("/opt/symforge/bin/symforge", {}, "linux"), "root/system");
});

test("classifyShadow flags a foreign user prefix", () => {
  assert.equal(
    classifyShadow("/home/other/.npm-global/bin/symforge", {}, "linux"),
    "foreign-prefix"
  );
});

test("classifyShadow never returns root/system on Windows", () => {
  // The /usr, /opt heuristics are POSIX-only; Windows always falls through.
  assert.equal(classifyShadow("C:\\tools\\symforge\\symforge.exe", {}, "win32"), "foreign-prefix");
});

test("isOwnShim recognizes a shim in the npm prefix bin dir as ours", (t) => {
  const root = tmpDir(t, "symforge-shim-prefix-");
  // Layout: <prefix>/lib/node_modules/symforge/bin (selfDir) + <prefix>/bin/symforge (shim)
  const selfDir = path.join(root, "lib", "node_modules", "symforge", "bin");
  fs.mkdirSync(selfDir, { recursive: true });
  // prefix = dirname(dirname(dirname(selfDir))) = root/lib
  const prefix = path.join(root, "lib");
  const shim = writeFakeBinary(path.join(prefix, "bin"), "symforge", "#!/bin/sh\nnode launcher\n");

  assert.equal(isOwnShim(shim, selfDir), true);
});

test("isOwnShim recognizes a shim by its launcher reference even from a foreign dir", (t) => {
  const root = tmpDir(t, "symforge-shim-content-");
  const selfDir = path.join(root, "lib", "node_modules", "symforge", "bin");
  fs.mkdirSync(selfDir, { recursive: true });
  const shimDir = path.join(root, "somewhere", "else");
  const shim = writeFakeBinary(
    shimDir,
    "symforge",
    'require("../lib/node_modules/symforge/bin/launcher.js")'
  );

  assert.equal(isOwnShim(shim, selfDir), true);
});

test("isOwnShim treats a genuinely foreign install as not ours", (t) => {
  const root = tmpDir(t, "symforge-shim-foreign-");
  const selfDir = path.join(root, "lib", "node_modules", "symforge", "bin");
  fs.mkdirSync(selfDir, { recursive: true });
  const foreign = writeFakeBinary(path.join(root, "usr", "local", "bin"), "symforge", "totally different binary");

  assert.equal(isOwnShim(foreign, selfDir), false);
});

test("launcher warns when a different symforge dir is first on PATH", (t) => {
  const root = tmpDir(t, "symforge-warn-");
  const shadowDir = path.join(root, "usr", "local", "bin");
  const shadow = writeFakeBinary(shadowDir, "symforge");
  // selfDir lives in a different prefix entirely.
  const selfDir = path.join(root, "home", "lib", "node_modules", "symforge", "bin");
  fs.mkdirSync(selfDir, { recursive: true });
  const resolvedBinary = path.join(root, "home", "lib", "node_modules", "symforge-linux-x64", "bin", "symforge");

  const errors = [];
  const launcher = createLauncher({
    process: {
      platform: "linux",
      arch: "x64",
      env: { PATH: shadowDir },
    },
    console: { error: (message) => errors.push(message) },
    selfDir,
    resolveBinary() {
      return { reason: "ok", binaryPath: resolvedBinary };
    },
    spawnSync() {
      return { status: 0 };
    },
  });

  assert.equal(launcher.main([]), 0);
  const joined = errors.join("\n");
  assert.match(joined, /different 'symforge'/);
  assert.match(joined, new RegExp(shadow.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
  assert.match(joined, /symforge init/);
});

test("launcher does NOT warn when its own npm shim is first on PATH", (t) => {
  const root = tmpDir(t, "symforge-noshim-warn-");
  // Realistic global install: <prefix>/lib/node_modules/symforge/bin is selfDir,
  // <prefix>/bin/symforge is the npm shim, and the native binary lives in the
  // sibling platform package.
  const prefix = path.join(root, "prefix");
  const selfDir = path.join(prefix, "lib", "node_modules", "symforge", "bin");
  fs.mkdirSync(selfDir, { recursive: true });
  const shimDir = path.join(prefix, "lib", "bin");
  const shim = writeFakeBinary(shimDir, "symforge", "#!/bin/sh\nnode launcher\n");
  const resolvedBinary = path.join(
    prefix,
    "lib",
    "node_modules",
    "symforge-linux-x64",
    "bin",
    "symforge"
  );

  const errors = [];
  const launcher = createLauncher({
    process: {
      platform: "linux",
      arch: "x64",
      env: { PATH: shimDir },
    },
    console: { error: (message) => errors.push(message) },
    selfDir,
    resolveBinary() {
      return { reason: "ok", binaryPath: resolvedBinary };
    },
    spawnSync() {
      return { status: 0 };
    },
  });

  assert.equal(launcher.main([]), 0);
  assert.deepEqual(errors, []);
  // Sanity: the shim really is the first PATH entry and a different path than
  // the resolved binary, so the "no warn" result is from shim recognition, not
  // an empty PATH.
  assert.notEqual(path.resolve(shim), path.resolve(resolvedBinary));
});

test("launcher does NOT warn when the resolved native binary is itself first on PATH", (t) => {
  const root = tmpDir(t, "symforge-self-warn-");
  const binDir = path.join(root, "bin");
  const resolvedBinary = writeFakeBinary(binDir, "symforge");
  const selfDir = path.join(root, "lib", "node_modules", "symforge", "bin");
  fs.mkdirSync(selfDir, { recursive: true });

  const errors = [];
  const launcher = createLauncher({
    process: {
      platform: "linux",
      arch: "x64",
      env: { PATH: binDir },
    },
    console: { error: (message) => errors.push(message) },
    selfDir,
    resolveBinary() {
      return { reason: "ok", binaryPath: resolvedBinary };
    },
    spawnSync() {
      return { status: 0 };
    },
  });

  assert.equal(launcher.main([]), 0);
  assert.deepEqual(errors, []);
});

test("SYMFORGE_NO_SHADOW_WARN suppresses the warning entirely", (t) => {
  const root = tmpDir(t, "symforge-suppress-");
  const shadowDir = path.join(root, "usr", "local", "bin");
  writeFakeBinary(shadowDir, "symforge");
  const selfDir = path.join(root, "home", "lib", "node_modules", "symforge", "bin");
  fs.mkdirSync(selfDir, { recursive: true });
  const resolvedBinary = path.join(root, "home", "lib", "node_modules", "symforge-linux-x64", "bin", "symforge");

  const errors = [];
  const launcher = createLauncher({
    process: {
      platform: "linux",
      arch: "x64",
      env: { PATH: shadowDir, SYMFORGE_NO_SHADOW_WARN: "1" },
    },
    console: { error: (message) => errors.push(message) },
    selfDir,
    resolveBinary() {
      return { reason: "ok", binaryPath: resolvedBinary };
    },
    spawnSync() {
      return { status: 0 };
    },
  });

  assert.equal(launcher.main([]), 0);
  assert.deepEqual(errors, []);
});

test("the shadow warning never writes to stdout", (t) => {
  const root = tmpDir(t, "symforge-stdout-");
  const shadowDir = path.join(root, "usr", "local", "bin");
  writeFakeBinary(shadowDir, "symforge");
  const selfDir = path.join(root, "home", "lib", "node_modules", "symforge", "bin");
  fs.mkdirSync(selfDir, { recursive: true });
  const resolvedBinary = path.join(root, "home", "lib", "node_modules", "symforge-linux-x64", "bin", "symforge");

  const stdoutChunks = [];
  const originalWrite = process.stdout.write;
  process.stdout.write = (chunk, ...rest) => {
    stdoutChunks.push(String(chunk));
    return originalWrite.call(process.stdout, chunk, ...rest);
  };
  t.after(() => {
    process.stdout.write = originalWrite;
  });

  // Real console.error is used here so it would hit stderr (not captured),
  // proving the warning path does not leak onto stdout.
  const launcher = createLauncher({
    process: {
      platform: "linux",
      arch: "x64",
      env: { PATH: shadowDir },
    },
    selfDir,
    resolveBinary() {
      return { reason: "ok", binaryPath: resolvedBinary };
    },
    spawnSync() {
      return { status: 0 };
    },
  });

  assert.equal(launcher.main([]), 0);
  assert.equal(stdoutChunks.join(""), "");
});

test("a thrown error inside the shadow check does not prevent spawn", (t) => {
  const selfDir = path.join(os.tmpdir(), "symforge-throw-self");
  const spawnCalls = [];

  // Force whichAllSymforge to throw by handing it a PATH getter that explodes.
  const explodingEnv = {};
  Object.defineProperty(explodingEnv, "PATH", {
    enumerable: true,
    get() {
      throw new Error("boom: PATH access exploded");
    },
  });

  const errors = [];
  const launcher = createLauncher({
    process: {
      platform: "linux",
      arch: "x64",
      env: explodingEnv,
    },
    console: { error: (message) => errors.push(message) },
    selfDir,
    resolveBinary() {
      return { reason: "ok", binaryPath: "/some/native/symforge" };
    },
    spawnSync(command, args, options) {
      spawnCalls.push({ command, args, options });
      return { status: 0 };
    },
  });

  // Spawn still happens and the exit status is returned despite the thrown
  // detection error.
  assert.equal(launcher.main(["--version"]), 0);
  assert.equal(spawnCalls.length, 1);
  assert.equal(spawnCalls[0].command, "/some/native/symforge");
});

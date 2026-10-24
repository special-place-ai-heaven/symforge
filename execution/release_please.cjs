'use strict';
const crypto = require('node:crypto');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {setTimeout: sleep} = require('node:timers/promises');

const PINS = Object.freeze({
  bundleUrl: 'https://raw.githubusercontent.com/googleapis/release-please-action/45996ed1f6d02564a971a2fa1b5860e934307cf7/dist/index.js',
  bundleSha256: '4fa35877a1c166013f0ecd7add1e6cf63ab44cf9082523dc96ce39212ae6a574',
  bundleBytes: 4303242,
  assets: Object.freeze({
    'commit.hbs': Object.freeze({bytes: 1194, sha256: '3b50b5b04d5067c0b00b768623c133ccf079c5b742d5ccd466ead967103d68d3'}),
    'commit1.hbs': Object.freeze({bytes: 670, sha256: '87b17ec541bcadb6771923f323c63b87ea436106578f37565b73e6afad904021'}),
    'footer.hbs': Object.freeze({bytes: 111, sha256: 'de814f47109202cb0f320a6474fb54a5ac6707454a894a205c544d5f1df607c0'}),
    'footer1.hbs': Object.freeze({bytes: 0, sha256: 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855'}),
    'header.hbs': Object.freeze({bytes: 173, sha256: '48e811c21c68c9ffe09c358c01535f96bd1ab4113563295dabe11845e41551d2'}),
    'header1.hbs': Object.freeze({bytes: 171, sha256: '2437dc15d41a68ed115b66e21a65ae3c6397f764ded3afd2dc28f6a8c19b8209'}),
    'template.hbs': Object.freeze({bytes: 115, sha256: 'e00ca3ed9c69a4c18802fd54acbe5898191d28495287a285b38c343747528d42'}),
    'template1.hbs': Object.freeze({bytes: 302, sha256: '6e2dba431e8f781dc9b3d21512f8f31a5c2a0c97244e350bcd9c9aeb977a57dc'}),
  }),
  graphqlUrl: 'https://api.github.com/graphql',
  querySha256: 'c2ffe7bbe70586013cc308745041fa9c37d689e950837f008fb6b9cc5188cc41',
});
const INTERNAL_MESSAGE = /^Something went wrong while executing your query on [0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z\. Please include `[0-9A-F]+(?::[0-9A-F]+){4}` when reporting this issue\.$/;
const sha256 = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);

function isCommitQuery(url, options, repository) {
  if (url !== PINS.graphqlUrl || options?.method !== 'POST' ||
      typeof options.body !== 'string' || options.body.length > 16384) return false;
  try {
    const body = JSON.parse(options.body), variables = body.variables;
    const [owner, repo, extra] = (repository || '').split('/');
    return owner && repo && !extra && object(body) && Object.keys(body).every(key => ['query', 'variables'].includes(key)) &&
      typeof body.query === 'string' && sha256(body.query) === PINS.querySha256 &&
      object(variables) && Object.keys(variables).every(key => ['owner', 'repo', 'num', 'maxFilesChanged', 'targetBranch', 'cursor'].includes(key)) &&
      variables.owner === owner && variables.repo === repo && variables.targetBranch === 'main' &&
      Number.isInteger(variables.num) && variables.num > 0 && variables.num <= 100 &&
      variables.maxFilesChanged === 100 && (variables.cursor === undefined || typeof variables.cursor === 'string');
  } catch {return false;}
}

async function readBounded(response, limit) {
  if (Number(response.headers.get('content-length')) > limit || !response.body) {
    void response.body?.cancel().catch(() => {});
    throw new Error('invalid response size');
  }
  const reader = response.body.getReader(), chunks = [];
  let bytes = 0;
  try {
    for (;;) {
      const {done, value} = await reader.read();
      if (done) break;
      bytes += value.byteLength;
      if (bytes > limit) throw new Error('response exceeds size limit');
      chunks.push(Buffer.from(value));
    }
    return Buffer.concat(chunks, bytes);
  } catch (error) {
    // A cloned response's cancellation can wait for its untouched original body.
    void reader.cancel().catch(() => {});
    throw error;
  } finally {reader.releaseLock();}
}

async function isInternalFailure(response) {
  if (response.status !== 200 || response.headers.has('retry-after') ||
      response.headers.get('x-ratelimit-remaining') === '0') return false;
  try {
    const payload = JSON.parse((await readBounded(response.clone(), 262144)).toString('utf8'));
    return object(payload) && Object.keys(payload).every(key => ['data', 'errors'].includes(key)) &&
      (payload.data === undefined || payload.data === null) &&
      Array.isArray(payload.errors) && payload.errors.length > 0 && payload.errors.every(error =>
        object(error) && Object.keys(error).every(key => ['message', 'type'].includes(key)) &&
        (error.type === undefined || error.type === 'INTERNAL') &&
        typeof error.message === 'string' && INTERNAL_MESSAGE.exec(error.message)?.[0] === error.message);
  } catch {return false;}
}

function createRetryFetch(fetchImpl, {repository, signal, sleep: wait = sleep}) {
  let retries = 0;
  return async (url, options) => {
    const signals = [signal, options?.signal].filter(Boolean);
    const requestSignal = signals.length ? AbortSignal.any(signals) : undefined;
    const requestOptions = signal ? {...options, signal: requestSignal} : options;
    const eligible = isCommitQuery(url, options, repository);
    for (;;) {
      requestSignal?.throwIfAborted();
      const response = await fetchImpl(url, requestOptions);
      requestSignal?.throwIfAborted();
      const retryable = eligible && retries < 2 && await isInternalFailure(response);
      requestSignal?.throwIfAborted();
      if (!retryable || retries >= 2) return response;
      const delay = 1000 * ++retries;
      await response.body?.cancel();
      await wait(delay, undefined, {signal: requestSignal});
    }
  };
}

async function readPinnedAsset(fetchImpl, url, pin, signal) {
  const response = await fetchImpl(url, {redirect: 'error', signal});
  if (!response.ok) throw new Error('action download failed');
  const bytes = await readBounded(response, pin.bytes);
  if (bytes.length !== pin.bytes || sha256(bytes) !== pin.sha256) throw new Error('action integrity check failed');
  return bytes;
}

async function loadAction({fetchImpl = globalThis.fetch, signal} = {}) {
  const downloadSignal = AbortSignal.any([AbortSignal.timeout(30000), ...(signal ? [signal] : [])]);
  const bytes = await readPinnedAsset(fetchImpl, PINS.bundleUrl,
    {bytes: PINS.bundleBytes, sha256: PINS.bundleSha256}, downloadSignal);
  const directory = fs.mkdtempSync(path.join(process.env.RUNNER_TEMP || os.tmpdir(), 'symforge-release-please-'));
  const filename = path.join(directory, 'action.cjs');
  const ownedFiles = [];
  const cleanup = () => {
    delete require.cache[filename];
    for (const file of ownedFiles) if (fs.existsSync(file)) fs.unlinkSync(file);
    fs.rmdirSync(directory);
  };
  const write = (file, contents) => {
    ownedFiles.push(file);
    fs.writeFileSync(file, contents, {flag: 'wx', mode: 0o600});
  };
  try {
    write(filename, bytes);
    // The upstream bundle reads its pinned changelog templates beside itself.
    for (const [name, pin] of Object.entries(PINS.assets)) {
      const asset = await readPinnedAsset(fetchImpl, new URL(name, PINS.bundleUrl).href, pin, downloadSignal);
      write(path.join(directory, name), asset);
    }
    const action = require(filename);
    if (typeof action.main !== 'function') throw new Error('action main export missing');
    return {main: action.main, cleanup};
  } catch (error) {cleanup(); throw error;}
}

async function runReleasePlease({fetchImpl = globalThis.fetch, signal, repository = process.env.GITHUB_REPOSITORY, load = loadAction} = {}) {
  const action = await load({fetchImpl, signal});
  try {
    if (typeof action.main !== 'function' || process.exitCode) throw new Error('action import failed');
    await action.main(createRetryFetch(fetchImpl, {repository, signal}));
    signal?.throwIfAborted();
    if (process.exitCode) throw new Error('action reported failure');
  } finally {action.cleanup();}
}

function reportFailure(_error, log = console.error) {
  log('::error::release-please failed; no action replay was attempted.');
  process.exitCode = 1;
}

module.exports = {PINS, createRetryFetch, readPinnedAsset, loadAction, runReleasePlease, reportFailure};
if (require.main === module) {
  const controller = new AbortController();
  const abort = () => controller.abort();
  process.once('SIGINT', abort);
  process.once('SIGTERM', abort);
  runReleasePlease({signal: controller.signal}).catch(reportFailure).finally(() => {
    process.removeListener('SIGINT', abort);
    process.removeListener('SIGTERM', abort);
  });
}

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

async function loadAction({fetchImpl = globalThis.fetch, signal} = {}) {
  const downloadSignal = AbortSignal.any([AbortSignal.timeout(30000), ...(signal ? [signal] : [])]);
  const response = await fetchImpl(PINS.bundleUrl, {redirect: 'error', signal: downloadSignal});
  if (!response.ok) throw new Error('action download failed');
  const bytes = await readBounded(response, PINS.bundleBytes);
  if (bytes.length !== PINS.bundleBytes || sha256(bytes) !== PINS.bundleSha256) throw new Error('action integrity check failed');
  const directory = fs.mkdtempSync(path.join(process.env.RUNNER_TEMP || os.tmpdir(), 'symforge-release-please-'));
  const filename = path.join(directory, 'action.cjs');
  const cleanup = () => {
    delete require.cache[filename];
    if (fs.existsSync(filename)) fs.unlinkSync(filename);
    fs.rmdirSync(directory);
  };
  try {
    fs.writeFileSync(filename, bytes, {flag: 'wx', mode: 0o600});
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

module.exports = {PINS, createRetryFetch, loadAction, runReleasePlease, reportFailure};
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

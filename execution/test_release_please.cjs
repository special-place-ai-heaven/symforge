'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {createRetryFetch, loadAction, runReleasePlease, reportFailure, PINS} = require('./release_please.cjs');
const QUERY = "query pullRequestsSince($owner: String!, $repo: String!, $num: Int!, $maxFilesChanged: Int, $targetBranch: String!, $cursor: String) {\n      repository(owner: $owner, name: $repo) {\n        ref(qualifiedName: $targetBranch) {\n          target {\n            ... on Commit {\n              history(first: $num, after: $cursor) {\n                nodes {\n                  associatedPullRequests(first: 10) {\n                    nodes {\n                      number\n                      title\n                      baseRefName\n                      headRefName\n                      labels(first: 10) {\n                        nodes {\n                          name\n                        }\n                      }\n                      body\n                      mergeCommit {\n                        oid\n                      }\n                      files(first: $maxFilesChanged) {\n                        nodes {\n                          path\n                        }\n                        pageInfo {\n                          endCursor\n                          hasNextPage\n                        }\n                      }\n                    }\n                  }\n                  sha: oid\n                  message\n                  author {\n                    name\n                    email\n                    user {\n                      login\n                    }\n                  }\n                }\n                pageInfo {\n                  hasNextPage\n                  endCursor\n                }\n              }\n            }\n          }\n        }\n      }\n    }";
const repository = 'special-place-ai-heaven/symforge';
const message = 'Something went wrong while executing your query on 2026-10-22T15:49:45Z. Please include `ABCD:123:ABCDEF:123456:ABCDEF12` when reporting this issue.';
function request(cursor) {
  return {method: 'POST', headers: {authorization: 'synthetic-test-placeholder'},
    body: JSON.stringify({query: QUERY, variables: {owner: 'special-place-ai-heaven', repo: 'symforge', num: 10, maxFilesChanged: 100, targetBranch: 'main', ...(cursor ? {cursor} : {})}})};
}
function response(payload = {errors: [{message}]}, status = 200, headers = {}) {
  return new Response(JSON.stringify(payload), {status, headers});
}
function harness(responses) {
  const calls = [], waits = [];
  const fetch = createRetryFetch(async (...args) => {calls.push(args); return responses.shift();},
    {repository, sleep: async ms => {waits.push(ms);}});
  return {fetch, calls, waits};
}
function releaseBlock() {
  const workflow = fs.readFileSync(path.join(__dirname, '../.github/workflows/release.yml'), 'utf8');
  return {workflow, block: workflow.split('      - id: release\n')[1].split('\n      # release-please skipping')[0]};
}
test('fixture pins the complete upstream query', () => {
  assert.equal(Buffer.byteLength(QUERY), 1586);
  assert.equal(require('node:crypto').createHash('sha256').update(QUERY).digest('hex'), PINS.querySha256);
});
test('retries the same read and preserves its successful response', async () => {
  const success = response({data: {repository: {ref: null}}});
  const h = harness([response(), response(), success]), options = request();
  assert.equal(await h.fetch(PINS.graphqlUrl, options), success);
  assert.equal(h.calls.length, 3);
  for (const [url, actual] of h.calls) {assert.equal(url, PINS.graphqlUrl); assert.equal(actual, options);}
  assert.deepEqual(h.waits, [1000, 2000]);
  assert.deepEqual(await success.json(), {data: {repository: {ref: null}}});
});
test('the two extra requests are shared across pagination calls', async () => {
  const final = response();
  const h = harness([response(), response({data: {ok: true}}), response(), response({data: {ok: true}}), final]);
  await h.fetch(PINS.graphqlUrl, request());
  await h.fetch(PINS.graphqlUrl, request('next-page'));
  assert.equal(await h.fetch(PINS.graphqlUrl, request('last-page')), final);
  assert.equal(h.calls.length, 5);
  assert.deepEqual(h.waits, [1000, 2000]);
  assert.deepEqual(await final.json(), {errors: [{message}]});
});
test('exhaustion returns the original final error body', async () => {
  const final = response();
  const h = harness([response(), response(), final]);
  assert.equal(await h.fetch(PINS.graphqlUrl, request()), final);
  assert.deepEqual(await final.json(), {errors: [{message}]});
});
test('permanent, mixed, unknown and malformed responses receive no retry', async () => {
  const cases = [
    response(undefined, 502), response(undefined, 401), response(undefined, 403), response(undefined, 429),
    response({errors: [{message: 'Bad credentials'}]}),
    response({errors: [{message}, {message: 'GraphQL validation failed'}]}),
    response({errors: [{message: message + '\n'}]}),
    response({errors: [{message: 'prefix ' + message}]}),
    response({errors: [{message, type: 'FORBIDDEN'}]}),
    response({errors: [{message, type: 'RATE_LIMITED'}]}),
    response({errors: [{message, type: 'UNKNOWN'}]}),
    response({errors: [{message, type: null}]}),
    response({errors: [{message, extensions: {code: 'UNAUTHENTICATED'}}]}),
    response({errors: [{message, extensions: {code: 'RATE_LIMITED'}}]}),
    response({errors: [{message, unknown: true}]}),
    response({data: {partial: true}, errors: [{message}]}),
    response({errors: []}), new Response('invalid JSON'),
    response(undefined, 200, {'retry-after': '1'}),
    response(undefined, 200, {'x-ratelimit-remaining': '0'}),
  ];
  for (const original of cases) {
    const h = harness([original]);
    assert.equal(await h.fetch(PINS.graphqlUrl, request()), original);
    assert.equal(h.calls.length, 1);
    assert.deepEqual(h.waits, []);
    assert.equal(typeof await original.text(), 'string');
  }
});
test('modified queries, variables, mutations, endpoints and REST are never replayed', async () => {
  const base = request(), parsed = JSON.parse(base.body);
  const cases = [
    ['https://example.invalid/graphql', base],
    [PINS.graphqlUrl, {...base, method: 'GET'}],
    [PINS.graphqlUrl, {...base, body: JSON.stringify({...parsed, query: QUERY.replace('query pullRequestsSince', 'mutation pullRequestsSince')})}],
    [PINS.graphqlUrl, {...base, body: JSON.stringify({...parsed, variables: {...parsed.variables, owner: 'other'}})}],
    [PINS.graphqlUrl, {...base, body: JSON.stringify({...parsed, variables: {...parsed.variables, arbitrary: true}})}],
    [PINS.graphqlUrl, {...base, body: JSON.stringify({...parsed, query: QUERY + '\n'})}],
    [PINS.graphqlUrl, {...base, body: JSON.stringify({...parsed, query: 'mutation { createRelease { id } }'})}],
    [PINS.graphqlUrl, {...base, body: JSON.stringify({...parsed, variables: {...parsed.variables, repo: 'other'}})}],
    [PINS.graphqlUrl, {...base, body: JSON.stringify({...parsed, variables: {...parsed.variables, targetBranch: 'other'}})}],
    ['https://api.github.com/repos/owner/repo/pulls', base],
  ];
  for (const args of cases) {
    const original = response(), h = harness([original]);
    assert.equal(await h.fetch(...args), original);
    assert.equal(h.calls.length, 1);
    assert.deepEqual(h.waits, []);
  }
});
test('transport failures propagate without replay', async () => {
  const failure = new Error('synthetic failure'); let calls = 0;
  const fetch = createRetryFetch(async () => {calls++; throw failure;}, {repository});
  await assert.rejects(fetch(PINS.graphqlUrl, request()), error => error === failure);
  assert.equal(calls, 1);
});
test('cancellation stops a retry wait and preserves the caller abort signal', async () => {
  const controller = new AbortController(); let calls = 0;
  const fetch = createRetryFetch(async () => {calls++; return response();}, {repository});
  const pending = fetch(PINS.graphqlUrl, {...request(), signal: controller.signal});
  setTimeout(() => controller.abort(), 20);
  await assert.rejects(pending, {name: 'AbortError'});
  assert.equal(calls, 1);
});
test('loader fails on network, HTTP, redirect, oversized and digest-invalid bodies', async () => {
  for (const fetchImpl of [
    async () => {throw new Error('synthetic failure');},
    async () => new Response('unavailable', {status: 503}),
    async () => new Response('redirect', {status: 302, headers: {location: 'https://example.invalid'}}),
    async () => new Response(new Uint8Array(PINS.bundleBytes + 1)),
    async () => new Response('invalid bundle'),
  ]) await assert.rejects(loadAction({fetchImpl}));
});
test('loader fixes the URL, forbids redirects and bounds its request lifetime', async () => {
  let calls = 0;
  await assert.rejects(loadAction({fetchImpl: async (url, options) => {
    calls++; assert.equal(url, PINS.bundleUrl); assert.equal(options.redirect, 'error');
    assert(options.signal instanceof AbortSignal);
    throw new Error('synthetic failure');
  }}));
  assert.equal(calls, 1);
});
test('main is invoked once, uses upstream outputs, and always cleans its owned dependency', async () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'symforge-release-test-'));
  const output = path.join(directory, 'output'); let calls = 0, cleanups = 0;
  try {
    await runReleasePlease({repository, load: async () => ({
      main: async fetch => {calls++; assert.equal(typeof fetch, 'function'); fs.writeFileSync(output, 'prs_created=true\nprs=[{"number":42}]\n');},
      cleanup: () => {cleanups++;}
    })});
    assert.equal(fs.readFileSync(output, 'utf8'), 'prs_created=true\nprs=[{"number":42}]\n');
    assert.equal(calls, 1); assert.equal(cleanups, 1);
  } finally {fs.rmSync(directory, {recursive: true});}
});
test('invalid exports, throws and SDK failure statuses remain failures without replay', async () => {
  for (const main of [undefined, async () => {throw new Error('synthetic failure');}, async () => {process.exitCode = 1;}]) {
    let cleanups = 0;
    try {await assert.rejects(runReleasePlease({repository, load: async () => ({main, cleanup: () => {cleanups++;}})}));}
    finally {process.exitCode = undefined;}
    assert.equal(cleanups, 1);
  }
});
test('fatal reporting never prints an error value', () => {
  const logs = [];
  try {reportFailure(new Error('synthetic-sensitive-value'), value => logs.push(value)); assert.equal(process.exitCode, 1);}
  finally {process.exitCode = undefined;}
  assert.deepEqual(logs, ['::error::release-please failed; no action replay was attempted.']);
});
test('workflow preserves every action input and downstream PR gates', () => {
  const {workflow, block} = releaseBlock();
  assert.match(block, /run: node execution\/release_please\.cjs/);
  for (const name of ['TOKEN','RELEASE-TYPE','PATH','TARGET-BRANCH','CONFIG-FILE','MANIFEST-FILE','REPO-URL','GITHUB-API-URL','GITHUB-GRAPHQL-URL','FORK','INCLUDE-COMPONENT-IN-TAG','PROXY-SERVER','SKIP-GITHUB-RELEASE','SKIP-GITHUB-PULL-REQUEST','SKIP-LABELING','CHANGELOG-HOST','VERSIONING-STRATEGY','RELEASE-AS']) assert(block.includes('INPUT_' + name + ':'));
  assert(block.includes("if: ${{ github.event_name != 'workflow_dispatch' || inputs.tag == '' }}"));
  assert(workflow.includes("steps.release.outputs.prs_created != 'true'"));
  assert(workflow.includes("steps.release.outputs.prs_created == 'true'"));
  assert(workflow.includes('RELEASE_PRS_JSON: ${{ steps.release.outputs.prs }}'));
  assert.match(workflow, /node-version: '24'/);
  assert(workflow.includes('node --test execution/test_release_please.cjs'));
});
test('concurrent pages reserve the shared retry budget before waiting', async () => {
  const h = harness(Array.from({length: 7}, () => response()));
  await Promise.all(Array.from({length: 5}, (_, i) => h.fetch(PINS.graphqlUrl, request(String(i)))));
  assert.equal(h.calls.length, 7);
  assert.deepEqual(h.waits, [1000, 2000]);
});
test('cancellation during a fetch cannot turn a response into success', async () => {
  const controller = new AbortController(); let calls = 0;
  const fetch = createRetryFetch(async () => {calls++; controller.abort(); return response({data: {ok: true}});},
    {repository, signal: controller.signal});
  await assert.rejects(fetch(PINS.graphqlUrl, request()), {name: 'AbortError'});
  assert.equal(calls, 1);
});
test('only the closed optional INTERNAL metadata shape can accompany the known message', async () => {
  const success = response({data: {ok: true}});
  const h = harness([response({errors: [{message, type: 'INTERNAL'}]}), success]);
  assert.equal(await h.fetch(PINS.graphqlUrl, request()), success);
  assert.deepEqual(h.waits, [1000]);
});
test('workflow values preserve upstream defaults as well as configured overrides', () => {
  const {block} = releaseBlock();
  const actual = Object.fromEntries([...block.matchAll(/^          (INPUT_[A-Z-]+): (.*)$/gm)].map(match => [match[1], match[2]]));
  assert(Object.hasOwn(actual, 'INPUT_TOKEN'));
  delete actual.INPUT_TOKEN;
  assert.deepEqual(actual, {
    'INPUT_RELEASE-TYPE': "''", INPUT_PATH: "''", 'INPUT_TARGET-BRANCH': "''",
    'INPUT_CONFIG-FILE': '.github/release-please-config.json',
    'INPUT_MANIFEST-FILE': '.github/.release-please-manifest.json',
    'INPUT_REPO-URL': '${{ github.repository }}', 'INPUT_GITHUB-API-URL': '${{ github.api_url }}',
    'INPUT_GITHUB-GRAPHQL-URL': '${{ github.graphql_url }}', INPUT_FORK: "'false'",
    'INPUT_INCLUDE-COMPONENT-IN-TAG': "'false'", 'INPUT_PROXY-SERVER': "''",
    'INPUT_SKIP-GITHUB-RELEASE': "'true'", 'INPUT_SKIP-GITHUB-PULL-REQUEST': "'false'",
    'INPUT_SKIP-LABELING': "'false'", 'INPUT_CHANGELOG-HOST': '${{ github.server_url }}',
    'INPUT_VERSIONING-STRATEGY': 'default', 'INPUT_RELEASE-AS': "''",
  });
});
function workflowSteps(workflow) {
  return workflow.match(/^      - [^\n]*(?:\n(?: {8}[^\n]*|))*/gm) || [];
}
function assertHistoricalRecoverySkips(workflow) {
  const condition = "if: ${{ github.event_name != 'workflow_dispatch' || inputs.tag == '' }}";
  const steps = workflowSteps(workflow);
  const testSteps = steps.filter(step => step.startsWith('      - name: Test release-please recovery contracts\n'));
  assert.equal(testSteps.length, 2);
  for (const step of testSteps) assert(step.includes(condition));
  const node24 = steps.find(step => step.startsWith('      - uses: actions/setup-node@') && step.includes("          node-version: '24'"));
  assert(node24?.includes(condition));
}
function assertCiRecoveryGate(workflow) {
  const job = workflow.split(/^  release-version:[ \t]*$/m)[1]?.split(/^  [A-Za-z0-9_-]+:[ \t]*$/m)[0];
  assert(job);
  assert(job.includes("node-version: '24'"));
  assert(job.includes('run: node --test execution/test_release_please.cjs'));
  assert(job.includes("python -m unittest discover -s execution -p 'test_*.py'"));
}
test('historical-tag rebuilds skip the new helper and its contract tests', () => {
  assertHistoricalRecoverySkips(releaseBlock().workflow);
});
test('the existing PR release-version gate runs recovery contracts on Node24', () => {
  assertCiRecoveryGate(fs.readFileSync(path.join(__dirname, '../.github/workflows/ci.yml'), 'utf8'));
});
test('workflow guards reject a borrowed step condition and a test in another job', () => {
  const {workflow} = releaseBlock();
  const node24 = workflowSteps(workflow).find(step =>
    step.startsWith('      - uses: actions/setup-node@') && step.includes("          node-version: '24'"));
  assert(node24);
  const unguarded = node24.replace(/^        if:[^\n]*\n/m, '');
  assert.notEqual(unguarded, node24);
  assert.throws(() => assertHistoricalRecoverySkips(workflow.replace(node24, unguarded)), {code: 'ERR_ASSERTION'});

  const contract = '      - run: node --test execution/test_release_please.cjs\n';
  const fixture = "jobs:\n  release-version:\n    steps:\n" +
    "      - uses: actions/setup-node@pinned\n        with:\n          node-version: '24'\n" +
    "      - run: python -m unittest discover -s execution -p 'test_*.py'\n" +
    contract + "  unrelated-check:\n    steps:\n";
  assertCiRecoveryGate(fixture);
  const misplaced = fixture.replace(contract, '') + contract;
  assert.throws(() => assertCiRecoveryGate(misplaced), {code: 'ERR_ASSERTION'});
});

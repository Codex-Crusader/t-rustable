'use strict';

const test = require('node:test');
const assert = require('node:assert');
const run = require('./cla.js');

const OWNER = 'Codex-Crusader';
const notFound = () => Object.assign(new Error('Not Found'), { status: 404 });

function commit(user, name = 'Someone') {
  return { author: user, commit: { author: { name } } };
}

// Fake GitHub API that records calls.
function fakeGitHub({ commits, signatures = null, comments = [] }) {
  const calls = [];
  const record = (name, result) => async (params) => {
    calls.push({ name, params });
    return { data: typeof result === 'function' ? result(params) : result };
  };
  const rest = {
    pulls: {
      listCommits: async () => ({ data: commits }),
      get: record('pulls.get', { number: 7, state: 'open', head: { sha: 'abc' } }),
    },
    issues: {
      listComments: async () => ({ data: comments }),
      createComment: record('issues.createComment', {}),
      updateComment: record('issues.updateComment', {}),
    },
    repos: {
      getContent: async () => {
        if (!signatures) throw notFound();
        return { data: { sha: 'filesha', content: Buffer.from(JSON.stringify(signatures)).toString('base64') } };
      },
      createOrUpdateFileContents: record('repos.createOrUpdateFileContents', {}),
      createCommitStatus: record('repos.createCommitStatus', {}),
    },
    git: {
      getRef: async () => {
        if (!signatures) throw notFound();
        return { data: {} };
      },
      createTree: record('git.createTree', { sha: 'treesha' }),
      createCommit: record('git.createCommit', { sha: 'commitsha' }),
      createRef: record('git.createRef', {}),
    },
  };
  const github = {
    rest,
    paginate: async (fn, params) => (await fn(params)).data,
  };
  return { github, calls };
}

const core = { info: () => {} };
const alice = { id: 1, login: 'alice', type: 'User' };

function prContext() {
  return {
    eventName: 'pull_request_target',
    repo: { owner: OWNER, repo: 't-rustable' },
    payload: { pull_request: { number: 7, state: 'open', head: { sha: 'abc' } } },
  };
}

function commentContext(user, body) {
  return {
    eventName: 'issue_comment',
    repo: { owner: OWNER, repo: 't-rustable' },
    payload: {
      issue: { number: 7, pull_request: {} },
      comment: { id: 99, user, body, created_at: '2026-10-16T00:00:00Z' },
    },
  };
}

const status = (calls) => calls.find((c) => c.name === 'repos.createCommitStatus').params.state;

test('unsigned author gives failure and a comment', async () => {
  const { github, calls } = fakeGitHub({ commits: [commit(alice)] });
  await run({ github, context: prContext(), core });
  assert.strictEqual(status(calls), 'failure');
  const comment = calls.find((c) => c.name === 'issues.createComment');
  assert.ok(comment.params.body.includes('@alice'));
});

test('first signature makes an orphan branch and gives success', async () => {
  const { github, calls } = fakeGitHub({ commits: [commit(alice)] });
  await run({ github, context: commentContext(alice, `  ${run.SIGN_PHRASE}\n`), core });
  const created = calls.find((c) => c.name === 'git.createCommit');
  assert.deepStrictEqual(created.params.parents, []);
  const tree = calls.find((c) => c.name === 'git.createTree').params.tree[0];
  assert.strictEqual(JSON.parse(tree.content)[0].login, 'alice');
  assert.strictEqual(status(calls), 'success');
});

test('later signature updates the existing file', async () => {
  const bob = { id: 2, login: 'bob', type: 'User' };
  const { github, calls } = fakeGitHub({
    commits: [commit(alice), commit(bob)],
    signatures: [{ id: 1, login: 'alice' }],
  });
  await run({ github, context: commentContext(bob, run.SIGN_PHRASE), core });
  const update = calls.find((c) => c.name === 'repos.createOrUpdateFileContents');
  assert.strictEqual(update.params.sha, 'filesha');
  const saved = JSON.parse(Buffer.from(update.params.content, 'base64').toString('utf8'));
  assert.deepStrictEqual(saved.map((s) => s.login), ['alice', 'bob']);
  assert.strictEqual(status(calls), 'success');
});

test('wrong phrase and non-authors are not recorded', async () => {
  const mallory = { id: 3, login: 'mallory', type: 'User' };
  for (const [user, body] of [[alice, 'I agree'], [mallory, run.SIGN_PHRASE]]) {
    const { github, calls } = fakeGitHub({ commits: [commit(alice)] });
    await run({ github, context: commentContext(user, body), core });
    assert.ok(!calls.some((c) => c.name.startsWith('git.') || c.name === 'repos.createOrUpdateFileContents'));
    assert.strictEqual(status(calls), 'failure');
  }
});

test('owner and bots do not need to sign', async () => {
  const owner = { id: 10, login: 'codex-crusader', type: 'User' };
  const bot = { id: 11, login: 'dependabot[bot]', type: 'Bot' };
  const { github, calls } = fakeGitHub({ commits: [commit(owner), commit(bot)] });
  await run({ github, context: prContext(), core });
  assert.strictEqual(status(calls), 'success');
  assert.ok(!calls.some((c) => c.name === 'issues.createComment'));
});

test('commit without a linked account gives failure', async () => {
  const { github, calls } = fakeGitHub({ commits: [commit(null, 'Unknown Person')] });
  await run({ github, context: prContext(), core });
  assert.strictEqual(status(calls), 'failure');
  const comment = calls.find((c) => c.name === 'issues.createComment');
  assert.ok(comment.params.body.includes('Unknown Person'));
});

test('existing bot comment is updated when all authors signed', async () => {
  const { github, calls } = fakeGitHub({
    commits: [commit(alice)],
    signatures: [{ id: 1, login: 'alice' }],
    comments: [{ id: 5, user: { type: 'Bot' }, body: '<!-- t-rustable-cla -->\nold' }],
  });
  await run({ github, context: prContext(), core });
  const update = calls.find((c) => c.name === 'issues.updateComment');
  assert.ok(update.params.body.includes('All commit authors signed'));
});

// CLA check for pull requests. Called from .github/workflows/cla.yml.
// Never reads or runs code from the pull request.
'use strict';

const CLA_VERSION = '1.0';
const SIGN_PHRASE = 'I have read the T-Rustable CLA version 1.0 in CLA.md and I agree to it.';
const BRANCH = 'cla-signatures';
const FILE = 'signatures/cla.json';
const MARKER = '<!-- t-rustable-cla -->';

const normalize = (text) => (text || '').trim().replace(/\s+/g, ' ');

async function readSignatures(github, owner, repo) {
  try {
    const { data } = await github.rest.repos.getContent({ owner, repo, path: FILE, ref: BRANCH });
    const list = JSON.parse(Buffer.from(data.content, 'base64').toString('utf8'));
    return { list, sha: data.sha };
  } catch (error) {
    if (error.status === 404) return { list: [], sha: undefined };
    throw error;
  }
}

async function branchExists(github, owner, repo) {
  try {
    await github.rest.git.getRef({ owner, repo, ref: `heads/${BRANCH}` });
    return true;
  } catch (error) {
    if (error.status === 404) return false;
    throw error;
  }
}

async function writeSignatures(github, owner, repo, signatures, login) {
  const content = JSON.stringify(signatures.list, null, 2) + '\n';
  const message = `chore(cla): record signature of @${login}`;

  if (await branchExists(github, owner, repo)) {
    await github.rest.repos.createOrUpdateFileContents({
      owner, repo, branch: BRANCH, path: FILE, message,
      content: Buffer.from(content, 'utf8').toString('base64'),
      sha: signatures.sha,
    });
    return;
  }

  // First signature: make an orphan branch that holds only the signatures file.
  const { data: tree } = await github.rest.git.createTree({
    owner, repo, tree: [{ path: FILE, mode: '100644', type: 'blob', content }],
  });
  const { data: commit } = await github.rest.git.createCommit({
    owner, repo, message, tree: tree.sha, parents: [],
  });
  await github.rest.git.createRef({ owner, repo, ref: `refs/heads/${BRANCH}`, sha: commit.sha });
}

async function commitAuthors(github, owner, repo, number) {
  const commits = await github.paginate(github.rest.pulls.listCommits, {
    owner, repo, pull_number: number, per_page: 100,
  });
  const authors = new Map();
  const unlinked = new Set();
  for (const c of commits) {
    const user = c.author;
    if (!user) {
      unlinked.add(c.commit.author.name);
    } else if (user.type !== 'Bot' && user.login.toLowerCase() !== owner.toLowerCase()) {
      authors.set(user.id, { id: user.id, login: user.login });
    }
  }
  return { authors, unlinked };
}

function commentBody(missing, unlinked) {
  if (missing.length === 0 && unlinked.size === 0) {
    return `${MARKER}\nAll commit authors signed the CLA. Thank you.`;
  }
  const lines = [
    MARKER,
    'Thank you for your contribution. Before we can merge it, each commit author must sign the Contributor License Agreement in `CLA.md`.',
    '',
  ];
  if (missing.length > 0) {
    lines.push('To sign, read `CLA.md`, then post a comment in this pull request with exactly this text:');
    lines.push('', '```', SIGN_PHRASE, '```', '');
    lines.push('Authors who did not sign yet: ' + missing.map((a) => `@${a.login}`).join(', '));
    lines.push('');
  }
  if (unlinked.size > 0) {
    lines.push('These commit authors have no GitHub account linked to their commit email: ' +
      [...unlinked].join(', ') + '. Add the email to a GitHub account, or change the commits to use a linked email.');
    lines.push('');
  }
  lines.push('Your GitHub user name and user ID are stored in public in the `cla-signatures` branch.');
  return lines.join('\n');
}

async function upsertComment(github, owner, repo, number, body, required) {
  const comments = await github.paginate(github.rest.issues.listComments, {
    owner, repo, issue_number: number, per_page: 100,
  });
  const existing = comments.find((c) => c.user && c.user.type === 'Bot' && (c.body || '').startsWith(MARKER));
  if (existing) {
    if (existing.body !== body) {
      await github.rest.issues.updateComment({ owner, repo, comment_id: existing.id, body });
    }
  } else if (required) {
    await github.rest.issues.createComment({ owner, repo, issue_number: number, body });
  }
}

async function run({ github, context, core }) {
  const { owner, repo } = context.repo;

  let pr;
  if (context.eventName === 'issue_comment') {
    if (!context.payload.issue.pull_request) return;
    ({ data: pr } = await github.rest.pulls.get({ owner, repo, pull_number: context.payload.issue.number }));
  } else {
    pr = context.payload.pull_request;
  }
  if (pr.state !== 'open') return;

  const { authors, unlinked } = await commitAuthors(github, owner, repo, pr.number);
  const signatures = await readSignatures(github, owner, repo);
  const signed = new Set(signatures.list.map((s) => s.id));

  if (context.eventName === 'issue_comment') {
    const c = context.payload.comment;
    if (normalize(c.body) === SIGN_PHRASE && authors.has(c.user.id) && !signed.has(c.user.id)) {
      signatures.list.push({
        id: c.user.id,
        login: c.user.login,
        cla_version: CLA_VERSION,
        pull_request: pr.number,
        comment_id: c.id,
        signed_at: c.created_at,
      });
      await writeSignatures(github, owner, repo, signatures, c.user.login);
      signed.add(c.user.id);
      core.info(`Recorded the signature of @${c.user.login}.`);
    }
  }

  const missing = [...authors.values()].filter((a) => !signed.has(a.id));
  const ok = missing.length === 0 && unlinked.size === 0;

  await github.rest.repos.createCommitStatus({
    owner, repo, sha: pr.head.sha, context: 'cla',
    state: ok ? 'success' : 'failure',
    description: ok ? 'All commit authors signed the CLA.' : 'One or more commit authors did not sign the CLA.',
  });
  await upsertComment(github, owner, repo, pr.number, commentBody(missing, unlinked), !ok);
}

module.exports = run;
module.exports.SIGN_PHRASE = SIGN_PHRASE;

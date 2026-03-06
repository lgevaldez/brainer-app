import test from 'node:test';
import assert from 'node:assert/strict';

import { formatWorkspaceKind, resolveTargetWorkspacesForAgents } from './workspaces.js';

test('resolveTargetWorkspacesForAgents filters non project-like folders', () => {
  const result = resolveTargetWorkspacesForAgents([
    { name: 'brainer', projectLike: true },
    { name: 'scratch', projectLike: false },
    { name: 'brainer-app', projectLike: true },
    { name: 'brainer', projectLike: true },
  ]);

  assert.deepEqual(result, ['brainer', 'brainer-app']);
});

test('formatWorkspaceKind falls back cleanly', () => {
  assert.equal(formatWorkspaceKind({ kind: 'git_repo', projectLike: true }), 'git_repo');
  assert.equal(formatWorkspaceKind({ projectLike: false }), 'folder');
});


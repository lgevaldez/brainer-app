import test from 'node:test';
import assert from 'node:assert/strict';

import { buildCognitiveSummary, formatCognitiveSummary } from './cognitive.js';

test('buildCognitiveSummary aggregates core counters', () => {
  const summary = buildCognitiveSummary(
    {
      counters: {
        'mcp.search_workspace_context': 4,
        'api.workspace.graph': 1,
        'memory.short_term.remember': 3,
        'memory.bundle.recall': 2,
      },
    },
    {
      queue_size: 2,
      recent_signals: [{}, {}],
    },
  );

  assert.equal(summary.searches, 4);
  assert.equal(summary.graphQueries, 1);
  assert.equal(summary.shortTermWrites, 3);
  assert.equal(summary.queueSize, 2);
  assert.equal(summary.recentSignals, 2);
});

test('formatCognitiveSummary renders stable lines', () => {
  const text = formatCognitiveSummary({
    searches: 1,
    graphQueries: 2,
    shortTermWrites: 3,
    recalls: 4,
    promotions: 5,
    checkpoints: 6,
    indexRuns: 7,
    backgroundQueued: 8,
    backgroundProcessedMemory: 9,
    backgroundProcessedIndex: 10,
    autopromoteAttempts: 11,
    queueSize: 12,
    recentSignals: 13,
  });

  assert.match(text, /searches: 1/);
  assert.match(text, /queue_size: 12/);
});

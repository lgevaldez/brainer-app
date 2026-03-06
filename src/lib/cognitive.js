function safeCounter(telemetry, name) {
  return Number(telemetry?.counters?.[name] || 0);
}

export function buildCognitiveSummary(telemetry, background) {
  const queued = Number(background?.queue_size || 0);
  const recentSignals = Array.isArray(background?.recent_signals) ? background.recent_signals.length : 0;

  return {
    searches: safeCounter(telemetry, 'mcp.search_workspace_context'),
    graphQueries: safeCounter(telemetry, 'mcp.get_graph_dependencies') + safeCounter(telemetry, 'api.workspace.graph'),
    shortTermWrites: safeCounter(telemetry, 'memory.short_term.remember'),
    recalls: safeCounter(telemetry, 'memory.bundle.recall'),
    promotions: safeCounter(telemetry, 'memory.short_term.promote'),
    checkpoints: safeCounter(telemetry, 'memory.short_term.checkpoint'),
    indexRuns: safeCounter(telemetry, 'index.run'),
    backgroundQueued: safeCounter(telemetry, 'background.queue.enqueued'),
    backgroundProcessedMemory: safeCounter(telemetry, 'background.processed.memory_intake'),
    backgroundProcessedIndex: safeCounter(telemetry, 'background.processed.index_run'),
    autopromoteAttempts: safeCounter(telemetry, 'background.memory.autopromote_attempted'),
    queueSize: queued,
    recentSignals,
  };
}

export function formatCognitiveSummary(summary) {
  return [
    `searches: ${summary.searches}`,
    `graph_queries: ${summary.graphQueries}`,
    `short_term_writes: ${summary.shortTermWrites}`,
    `recalls: ${summary.recalls}`,
    `promotions: ${summary.promotions}`,
    `checkpoints: ${summary.checkpoints}`,
    `index_runs: ${summary.indexRuns}`,
    `background_enqueued: ${summary.backgroundQueued}`,
    `background_processed_memory: ${summary.backgroundProcessedMemory}`,
    `background_processed_index: ${summary.backgroundProcessedIndex}`,
    `autopromote_attempts: ${summary.autopromoteAttempts}`,
    `queue_size: ${summary.queueSize}`,
    `recent_signals: ${summary.recentSignals}`,
  ].join('\n');
}


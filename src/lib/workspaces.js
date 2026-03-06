export function isProjectLikeEntry(entry) {
  return Boolean(entry && entry.projectLike);
}

export function resolveTargetWorkspacesForAgents(entries) {
  return Array.from(
    new Set(
      (entries || [])
        .filter((entry) => isProjectLikeEntry(entry))
        .map((entry) => String(entry?.name || '').trim())
        .filter((name) => name.length > 0),
    ),
  );
}

export function formatWorkspaceKind(entry) {
  if (!entry) return 'unknown';
  return entry.kind || (entry.projectLike ? 'project' : 'folder');
}


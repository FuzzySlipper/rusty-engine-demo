export const activeGuidancePaths = new Set([
  "AGENTS.md",
  "README.md",
  "docs/agent-code-atlas.md",
  "docs/design.md",
  "docs/extension-recipes.md",
  "docs/fps-product-architecture.md",
  "docs/game-session-protocol.md",
  "docs/studio-adapter.md",
  "docs/tauri-desktop.md",
  "docs/weapon-authoring-contract.md",
]);

const retiredEngineGuidance = [
  [
    "pinned or exact-revision Engine topology",
    /(?:\b(?:pinned|exact[- ]pinned|exact[- ]revision)\b[^\n]{0,120}\b(?:Rusty\s+)?Engine\b|\b(?:Rusty\s+)?Engine\b[^\n]{0,120}\b(?:pinned|exact[- ]pinned|exact[- ]revision)\b)/iu,
  ],
  [
    "floating Engine revision topology",
    /\bfloating\s+(?:Rusty\s+)?Engine\s+revisions?\b/iu,
  ],
  [
    "sibling-path prohibition",
    /(?:\b(?:forbid(?:den)?|reject(?:ed)?|prohibit(?:ed)?|must\s+not|do\s+not|never|wrong)\b[^\n]{0,180}\bsibling[ -]paths?\b|\bsibling[ -]paths?\b[^\n]{0,180}\b(?:forbid(?:den)?|reject(?:ed)?|prohibit(?:ed)?|must\s+not|do\s+not|never|wrong)\b)/iu,
  ],
];

export function auditActiveGuidance(relativePath, content) {
  if (!activeGuidancePaths.has(relativePath)) return [];
  return retiredEngineGuidance.flatMap(([label, pattern]) => {
    const match = content.match(pattern);
    return match === null ? [] : [{ label, excerpt: match[0] }];
  });
}

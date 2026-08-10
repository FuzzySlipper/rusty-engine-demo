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

const engine = /\b(?:Rusty\s+)?Engine\b/iu;
const gitIdentity =
  /\b(?:revisions?|commit|shas?|git\s+(?:identity|ref)|tags?|branches?|public\s+main|main\s+branch)\b/iu;
const identityAuthority =
  /\b(?:exact|floating|use|uses|using|require|required|requires|resolve|resolves|select|selects|tie|tied|fix|fixed|set|sets|lock|locks|locked|pin|pins|pinned|refresh|sync|synchronize|update|fetch|pull|follow)\b/iu;
const pinOrLock = /\b(?:pin|pins|pinned|lock|locks|locked)\b/iu;
const checkoutSynchronization =
  /\b(?:refresh|sync|synchronize|update|fetch|pull|follow)\b/iu;
const checkoutOrMain =
  /\b(?:checkout|public\s+main|main\s+branch|branch\s+main)\b/iu;
const freshness = /\bfreshness\b/iu;
const freshCheckout = /\bfresh\b/iu;
const concreteGitIdentity = /\b[0-9a-f]{40}\b/iu;
const antiCeremony =
  /\b(?:no|without|must\s+not|do\s+not|does\s+not|is\s+not|not\s+as|never)\b[^.!?\n]{0,220}\b(?:fetch|manage|mutate|certify|add|exact|floating|use|require|resolve|select|tie|fix|set|lock|pin|refresh|sync|synchronize|update|pull|follow|freshness|revision|commit|sha|git\s+(?:identity|ref)|tag|branch)\b/iu;
const historicalScope =
  /\b(?:historical|predecessor|then-(?:current|reviewed)|proven\s+upstream\s+gaps|evidence\s+(?:used|recorded)|reviewed\s+at|approved\s+at)\b/iu;
const currentDirective =
  /\b(?:must|should|required|requires?|use|builds?|before\s+building|refresh|sync|synchronize|resolve|lock|update|pull|follow)\b/iu;
const siblingPathProhibition =
  /(?:\b(?:forbid(?:den)?|reject(?:ed)?|prohibit(?:ed)?|must\s+not|do\s+not|never|wrong)\b[^\n]{0,180}\bsibling[ -]paths?\b|\bsibling[ -]paths?\b[^\n]{0,180}\b(?:forbid(?:den)?|reject(?:ed)?|prohibit(?:ed)?|must\s+not|do\s+not|never|wrong)\b)/iu;

export function auditActiveGuidance(relativePath, content) {
  if (!activeGuidancePaths.has(relativePath)) return [];
  const findings = [];
  for (const excerpt of guidanceStatements(content)) {
    if (excerpt === "") continue;
    const siblingMatch = excerpt.match(siblingPathProhibition);
    if (siblingMatch !== null) {
      findings.push({
        label: "sibling-path prohibition",
        excerpt: siblingMatch[0],
      });
    }
    if (!engine.test(excerpt)) continue;
    const historicallyScoped =
      historicalScope.test(excerpt) && !currentDirective.test(excerpt);
    if (historicallyScoped || antiCeremony.test(excerpt)) continue;
    const tiesEngineToGitIdentity =
      gitIdentity.test(excerpt) &&
      (identityAuthority.test(excerpt) ||
        (concreteGitIdentity.test(excerpt) && currentDirective.test(excerpt)));
    const managesEngineCheckout =
      checkoutSynchronization.test(excerpt) && checkoutOrMain.test(excerpt);
    const requiresFreshCheckout =
      freshCheckout.test(excerpt) &&
      checkoutOrMain.test(excerpt) &&
      currentDirective.test(excerpt);
    if (
      pinOrLock.test(excerpt) ||
      freshness.test(excerpt) ||
      requiresFreshCheckout ||
      tiesEngineToGitIdentity ||
      managesEngineCheckout
    ) {
      findings.push({
        label: "Engine revision, Git identity, or checkout ceremony",
        excerpt,
      });
    }
  }
  return findings;
}

function guidanceStatements(content) {
  const statements = [];
  for (const paragraph of content.split(/\n\s*\n/u)) {
    const lines = paragraph
      .split("\n")
      .map((line) => line.trim())
      .filter((line) => line !== "");
    if (lines.length === 0) continue;
    const units = [];
    if (lines.some((line) => line.startsWith("|"))) {
      units.push(...lines);
    } else if (lines.some((line) => /^(?:[-*]|\d+[.)])\s+/u.test(line))) {
      let current = "";
      for (const line of lines) {
        if (/^(?:[-*]|\d+[.)])\s+/u.test(line) && current !== "") {
          units.push(current);
          current = line;
        } else {
          current = current === "" ? line : `${current} ${line}`;
        }
      }
      if (current !== "") units.push(current);
    } else {
      units.push(lines.join(" "));
    }
    for (const unit of units) {
      statements.push(...unit.split(/(?<=[.!?])\s+(?=[A-Z`])/u));
    }
  }
  return statements;
}

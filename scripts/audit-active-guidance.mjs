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
  /\b(?:revisions?|commits?(?!\s+(?:a|an|the)\b)|shas?|git\s+(?:identity|ref)|tags?|branches?|public\s+main|main\s+branch)\b/iu;
const pinOrLock = /\b(?:pin|pins|pinned|lock|locks|locked)\b/iu;
const checkout = /\bcheckout\b/iu;
const checkoutCeremony =
  /\b(?:fresh|freshness|refresh|sync|synchronize|match|update|fetch|pull|follow)\b/iu;
const freshness = /\bfreshness\b/iu;
const plainGitIdentityListItem =
  /^\s*(?:(?:the|exact|current|historical|reviewed)\s+)?(?:revisions?|commits?|shas?|git\s+(?:identity|ref)|tags?|branches?|public\s+main|main\s+branch)(?:\s+provenance)?[.,:]?\s*$/iu;
const antiCeremony =
  /(?:\b(?:no|without|must\s+not|do\s+not|does\s+not|is\s+not|not\s+as|never)\b[^,;.!?\n]{0,120}\b(?:add|use|fetch|manage|mutate|certify|lock|pin|refresh|sync|synchronize|match|update|pull|follow|freshness|revision|commit|sha|git\s+(?:identity|ref)|tag|branch)\b|\b(?:revision|commit|sha|git\s+(?:identity|ref)|tag|branch|freshness|refresh|sync|synchronization|update|lock|pin)\b[^,;.!?\n]{0,80}\b(?:is|are)\s+not\b)/iu;
const historicalScope =
  /\b(?:historical|predecessor|then-(?:current|reviewed)|proven\s+upstream\s+gaps?|evidence\s+(?:used|recorded)|reviewed\s+at|approved\s+at)\b/iu;
const siblingPathProhibition =
  /(?:\b(?:forbid(?:den)?|reject(?:ed)?|prohibit(?:ed)?|must\s+not|do\s+not|never|wrong)\b[^\n]{0,180}\bsibling[ -]paths?\b|\bsibling[ -]paths?\b[^\n]{0,180}\b(?:forbid(?:den)?|reject(?:ed)?|prohibit(?:ed)?|must\s+not|do\s+not|never|wrong)\b)/iu;

export function auditActiveGuidance(relativePath, content) {
  if (!activeGuidancePaths.has(relativePath)) return [];
  const findings = [];
  for (const excerpt of guidanceClauses(content)) {
    if (excerpt === "") continue;
    const siblingMatch = excerpt.match(siblingPathProhibition);
    if (siblingMatch !== null) {
      findings.push({
        label: "sibling-path prohibition",
        excerpt: siblingMatch[0],
      });
    }
    if (!engine.test(excerpt)) continue;
    if (historicalScope.test(excerpt) || antiCeremony.test(excerpt)) continue;
    const tiesEngineToGitIdentity = gitIdentity.test(excerpt);
    const managesEngineCheckout =
      checkout.test(excerpt) && checkoutCeremony.test(excerpt);
    if (
      pinOrLock.test(excerpt) ||
      freshness.test(excerpt) ||
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

function guidanceClauses(content) {
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
      for (const sentence of unit.split(/(?<=[.!?])\s+(?=[A-Z`])/u)) {
        const clauses = sentence
          .split(
            /\s*;\s*|\s*,\s*(?=(?:but|yet|however|while)\b)|\s+(?:and|or)\s+/iu,
          )
          .reduce((classified, clause) => {
            if (
              classified.length > 0 &&
              plainGitIdentityListItem.test(clause)
            ) {
              classified[classified.length - 1] += ` and ${clause}`;
            } else {
              classified.push(clause);
            }
            return classified;
          }, []);
        const governedByEngine = engine.test(sentence);
        statements.push(
          ...clauses.map((clause) =>
            governedByEngine && !engine.test(clause)
              ? `Engine ${clause}`
              : clause,
          ),
        );
      }
    }
  }
  return statements;
}

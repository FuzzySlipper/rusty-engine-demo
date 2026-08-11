import { createHash } from "node:crypto";

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

export function certifySchemaOnlyMigration(projectBytes, project, evidence) {
  const source = projectBytes.toString("utf8");
  const currentMarker = `"schemaVersion": ${String(evidence.toSchemaVersion)},`;
  const previousMarker = `"schemaVersion": ${String(evidence.fromSchemaVersion)},`;
  if (
    project.schemaVersion !== evidence.toSchemaVersion ||
    evidence.changedFields.join(",") !== "schemaVersion" ||
    source.split(currentMarker).length !== 2 ||
    projectBytes.byteLength !== evidence.finalBytes ||
    sha256(projectBytes) !== evidence.finalHash
  ) {
    return false;
  }
  const previousBytes = Buffer.from(source.replace(currentMarker, previousMarker));
  return (
    previousBytes.byteLength === evidence.startingBytes &&
    sha256(previousBytes) === evidence.startingHash
  );
}

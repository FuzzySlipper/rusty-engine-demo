import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { createHash } from "node:crypto";

test("desktop package manifest is canonical, sorted, and hash-complete", () => {
  const path = new URL(
    "../src-tauri/resources/desktop-package-manifest.json",
    import.meta.url,
  );
  const manifest = JSON.parse(readFileSync(path, "utf8"));
  assert.equal(manifest.schemaVersion, 1);
  assert.match(manifest.sourceRevision, /^[0-9a-f]{40}$/);
  assert.ok(manifest.files.length > 10);
  assert.deepEqual(
    manifest.files.map(({ path }) => path),
    manifest.files.map(({ path }) => path).toSorted(),
  );
  assert.equal(
    manifest.files.filter(({ kind }) => kind === "sidecar").length,
    1,
  );
  for (const file of manifest.files) {
    assert.match(file.path, /^(content|web|loading-bay-browser-host)/);
    assert.doesNotMatch(file.path, /(^|\/)\.\.(\/|$)/);
    assert.match(file.sha256, /^[0-9a-f]{64}$/);
    assert.ok(file.byteLen > 0);
  }
});

test("sha256 evidence format detects byte changes", () => {
  const original = Buffer.from("loading-bay");
  const changed = Buffer.from("loading-bay!");
  assert.notEqual(
    createHash("sha256").update(original).digest("hex"),
    createHash("sha256").update(changed).digest("hex"),
  );
});

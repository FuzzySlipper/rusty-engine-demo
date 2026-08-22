import test from "node:test";
import assert from "node:assert/strict";
import {
  RUSTY_STANDARD_HOST_WIRE_SCHEMAS,
  RustyDeveloperCommandClientError,
  createRustyDeveloperCommandClient,
  type RustyDeveloperCommandAdapter,
} from "@rusty-engine/application-host";
import { LOADING_BAY_DEVELOPER_COMMAND_EXTENSION } from "./developer-command.ts";

const discovery = {
  protocolVersion: 1,
  runtime: "loading-bay-test",
  profile: "loading-bay.developer",
  permittedLanes: ["inspect", "play", "admin"],
  revision: "1",
  catalogEpoch: "1",
  contractFingerprint: "loading-bay.test",
  commands: [
    {
      id: "standard.inspect.entity",
      aliases: [],
      lane: "inspect",
      summary: "Inspect an entity.",
    },
    {
      id: "standard.inspect.mechanics",
      aliases: [],
      lane: "inspect",
      summary: "Inspect mechanics.",
    },
    {
      id: "standard.admin.track.set",
      aliases: [],
      lane: "admin",
      summary: "Set a track.",
    },
    {
      id: "loading-bay.play.service-command",
      aliases: [],
      lane: "play",
      summary: "Queue product input.",
    },
  ],
};

function adapter(snapshot = discovery): RustyDeveloperCommandAdapter {
  return {
    discover: async () => snapshot,
    execute: async (request) => ({
      correlation: request.correlation,
      runtime: snapshot.runtime,
      profile: snapshot.profile,
      revision: snapshot.revision,
      catalogEpoch: snapshot.catalogEpoch,
      outcome: {
        kind: "success",
        value: {
          kind: "CommandConsumed",
          connectionGeneration: 1,
          commandSequence: 2,
        },
        receiptRefs: [],
      },
    }),
  };
}

test("schema-only Loading Bay binding reconciles with Rust discovery and validates strict results", async () => {
  const client = createRustyDeveloperCommandClient({
    adapter: adapter(),
    schemas: RUSTY_STANDARD_HOST_WIRE_SCHEMAS,
    extensions: [LOADING_BAY_DEVELOPER_COMMAND_EXTENSION],
    createCorrelation: () => "loading-bay-product-command",
  });

  await client.discover();
  assert.equal(
    client.descriptor("loading-bay.play.service-command")?.lane,
    "play",
  );
  const response = await client.execute("loading-bay.play.service-command", {
    kind: "setInputIntent",
  });
  assert.equal(response.outcome.kind, "success");
  assert.deepEqual(response.outcome, {
    kind: "success",
    value: {
      kind: "CommandConsumed",
      connectionGeneration: 1,
      commandSequence: 2,
    },
    receiptRefs: [],
  });
  client.dispose();
});

test("schema-only Loading Bay binding rejects missing or drifted Rust discovery", async () => {
  const missing = { ...discovery, commands: discovery.commands.slice(0, -1) };
  const client = createRustyDeveloperCommandClient({
    adapter: adapter(missing),
    schemas: RUSTY_STANDARD_HOST_WIRE_SCHEMAS,
    extensions: [LOADING_BAY_DEVELOPER_COMMAND_EXTENSION],
  });
  await assert.rejects(
    client.discover(),
    (error: unknown) =>
      error instanceof RustyDeveloperCommandClientError &&
      error.code === "invalid_extension" &&
      error.message.includes("has no available discovered command"),
  );

  const laneDrift = {
    ...discovery,
    commands: discovery.commands.map((command) =>
      command.id === "loading-bay.play.service-command"
        ? { ...command, lane: "admin" }
        : command,
    ),
  };
  const driftedClient = createRustyDeveloperCommandClient({
    adapter: adapter(laneDrift),
    schemas: RUSTY_STANDARD_HOST_WIRE_SCHEMAS,
    extensions: [LOADING_BAY_DEVELOPER_COMMAND_EXTENSION],
  });
  await assert.rejects(
    driftedClient.discover(),
    (error: unknown) =>
      error instanceof RustyDeveloperCommandClientError &&
      error.code === "invalid_extension" &&
      error.message.includes("expects lane play"),
  );
});

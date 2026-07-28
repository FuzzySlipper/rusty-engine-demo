import "@angular/compiler";

import assert from "node:assert/strict";
import test from "node:test";

import {
  STUDIO_ADAPTER_OPERATIONS,
  STUDIO_ADAPTER_PROTOCOL_VERSION,
  type AdapterDescription,
  type StudioEntityComponentReference,
} from "@rusty-engine/studio-adapter-client";
import {
  admitStudioEntityInspectorContributions,
  matchStudioEntityInspectorContributions,
} from "@rusty-engine/studio-editor-shell";

import { LOADING_BAY_WEAPON_INSPECTOR_CONTRIBUTION } from "./loading-bay-weapon-inspector-panel.component.js";
import {
  LOADING_BAY_WEAPON_AUTHORING_CONTRACT_ID,
  LOADING_BAY_WEAPON_COMPONENT_TYPE_ID,
} from "./weapon-authoring-codec.js";

test("Loading Bay contribution mounts only for the exact advertised v1 weapon contract", () => {
  const contributions = admitStudioEntityInspectorContributions([
    LOADING_BAY_WEAPON_INSPECTOR_CONTRIBUTION,
  ]);
  const references: StudioEntityComponentReference[] = [
    weaponReference(1),
    weaponReference(2),
    {
      ownerEntityId: 88,
      componentTypeId: "rusty-engine-demo.loading-bay.unknown",
      inspectorContract: null,
    },
  ];
  const matches = matchStudioEntityInspectorContributions(
    contributions,
    references,
    adapter(),
    88,
  );

  assert.equal(Object.isFrozen(contributions), true);
  assert.deepEqual(
    matches.map(({ contribution, reference }) => ({
      componentTypeId: reference.componentTypeId,
      contract: contribution.contract,
      dataVisualId: contribution.dataVisualId,
    })),
    [
      {
        componentTypeId: LOADING_BAY_WEAPON_COMPONENT_TYPE_ID,
        contract: {
          contractId: LOADING_BAY_WEAPON_AUTHORING_CONTRACT_ID,
          contractVersion: 1,
        },
        dataVisualId: "loading-bay-weapon-component",
      },
    ],
  );
  assert.equal(
    references.length - matches.length,
    2,
    "unsupported version and unknown identity remain host-owned read-only rows",
  );
});

function weaponReference(
  contractVersion: number,
): StudioEntityComponentReference {
  return {
    ownerEntityId: 88,
    componentTypeId: LOADING_BAY_WEAPON_COMPONENT_TYPE_ID,
    inspectorContract: {
      contractId: LOADING_BAY_WEAPON_AUTHORING_CONTRACT_ID,
      contractVersion,
    },
  };
}

function adapter(): AdapterDescription {
  return {
    adapterId: "rusty-engine-demo.loading-bay",
    adapterVersion: 10,
    protocolVersion: STUDIO_ADAPTER_PROTOCOL_VERSION,
    projectKind: "loadingBayProject",
    projectSchemaVersion: 21,
    operations: STUDIO_ADAPTER_OPERATIONS,
    entityInspectorContracts: [
      {
        contractId: LOADING_BAY_WEAPON_AUTHORING_CONTRACT_ID,
        contractVersion: 1,
      },
    ],
  };
}

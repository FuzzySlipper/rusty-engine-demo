export const LOADING_BAY_WEAPON_COMPONENT_TYPE_ID =
  "rusty-engine-demo.loading-bay.weapon";
export const LOADING_BAY_WEAPON_AUTHORING_CONTRACT_ID =
  "rusty-engine-demo.loading-bay.weapon-authoring";
export const LOADING_BAY_WEAPON_AUTHORING_CONTRACT_VERSION = 1;
export const MAX_LOADING_BAY_WEAPON_AUTHORING_REQUEST_BYTES = 16 * 1024;
export const MAX_LOADING_BAY_WEAPON_AUTHORING_RESPONSE_BYTES = 32 * 1024;

export type LoadingBayWeaponAttackMode =
  | { readonly mode: "hitscan" }
  | {
      readonly mode: "spread";
      readonly pelletCount: number;
      readonly spreadDegrees: number;
    }
  | { readonly mode: "automatic" };

export interface LoadingBayWeaponCandidate {
  readonly attackMode: LoadingBayWeaponAttackMode;
  readonly damage: number;
  readonly maxDistance: number;
  readonly cooldownTicks: number;
  readonly ammunitionItemId: string;
  readonly ammunitionCost: number;
  readonly muzzleOffset: readonly [number, number, number];
  readonly presentation: string;
}

export interface LoadingBayWeaponBinding {
  readonly inventoryOwnerEntityId: number;
  readonly slotIndex: number;
  readonly startingQuantity: number;
  readonly initiallyEquipped: boolean;
}

export interface LoadingBayWeaponReadout {
  readonly componentTypeId: typeof LOADING_BAY_WEAPON_COMPONENT_TYPE_ID;
  readonly contractId: typeof LOADING_BAY_WEAPON_AUTHORING_CONTRACT_ID;
  readonly contractVersion: typeof LOADING_BAY_WEAPON_AUTHORING_CONTRACT_VERSION;
  readonly ownerEntityId: number;
  readonly componentRevision: string;
  readonly itemDefinitionId: string;
  readonly binding: LoadingBayWeaponBinding;
  readonly definition: LoadingBayWeaponCandidate;
}

export interface LoadingBayWeaponReceipt {
  readonly ownerEntityId: number;
  readonly itemDefinitionId: string;
  readonly projectHashBefore: string;
  readonly projectHashAfter: string;
  readonly componentRevisionBefore: string;
  readonly componentRevisionAfter: string;
}

export type LoadingBayWeaponRejectionCode =
  | "unsupportedContractVersion"
  | "invalidRequestId"
  | "invalidProjectHash"
  | "staleProject"
  | "projectRejected"
  | "weaponNotFound"
  | "staleComponent"
  | "candidateRejected"
  | "projectStoreFailure";

export interface LoadingBayWeaponRejection {
  readonly code: LoadingBayWeaponRejectionCode;
  readonly message: string;
  readonly path?: string;
}

export type LoadingBayWeaponAuthoringResponse =
  | {
      readonly type: "loadingBayWeaponRead";
      readonly contractVersion: 1;
      readonly requestId: string;
      readonly weapon: LoadingBayWeaponReadout;
    }
  | {
      readonly type: "loadingBayWeaponReplaced";
      readonly contractVersion: 1;
      readonly requestId: string;
      readonly receipt: LoadingBayWeaponReceipt;
      readonly weapon: LoadingBayWeaponReadout;
    }
  | {
      readonly type: "loadingBayWeaponRejected";
      readonly contractVersion: 1;
      readonly requestId: string;
      readonly rejection: LoadingBayWeaponRejection;
    };

export class LoadingBayWeaponProtocolError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "LoadingBayWeaponProtocolError";
  }
}

export class LoadingBayWeaponOperationRejected extends Error {
  readonly requestId: string;
  readonly rejection: LoadingBayWeaponRejection;

  constructor(requestId: string, rejection: LoadingBayWeaponRejection) {
    super(rejection.message);
    this.name = "LoadingBayWeaponOperationRejected";
    this.requestId = requestId;
    this.rejection = rejection;
  }
}

export interface LoadingBayWeaponAuthoringPort {
  readonly exchange: (request: string, signal: AbortSignal) => Promise<string>;
}

export type LoadingBayWeaponFetch = (
  input: string,
  init: RequestInit,
) => Promise<Pick<Response, "headers" | "ok" | "status" | "text">>;

export class LoadingBayWeaponTransportError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "LoadingBayWeaponTransportError";
  }
}

export class HttpLoadingBayWeaponAuthoringPort
  implements LoadingBayWeaponAuthoringPort
{
  readonly #endpoint: string;
  readonly #fetch: LoadingBayWeaponFetch;

  constructor(
    endpoint = "/api/studio-adapter",
    fetchImplementation: LoadingBayWeaponFetch = globalThis.fetch.bind(
      globalThis,
    ),
  ) {
    this.#endpoint = endpoint;
    this.#fetch = fetchImplementation;
  }

  async exchange(request: string, signal: AbortSignal): Promise<string> {
    const requestBytes = new TextEncoder().encode(request).byteLength;
    if (requestBytes > MAX_LOADING_BAY_WEAPON_AUTHORING_REQUEST_BYTES) {
      throw new LoadingBayWeaponTransportError(
        `weapon authoring request is ${String(requestBytes)} bytes, exceeding the ${String(
          MAX_LOADING_BAY_WEAPON_AUTHORING_REQUEST_BYTES,
        )}-byte bound`,
      );
    }
    const response = await this.#fetch(this.#endpoint, {
      method: "POST",
      headers: {
        accept: "application/json",
        "content-type": "application/json",
      },
      body: request,
      signal,
    });
    const declaredLength = response.headers.get("content-length");
    if (
      declaredLength !== null &&
      Number(declaredLength) > MAX_LOADING_BAY_WEAPON_AUTHORING_RESPONSE_BYTES
    ) {
      throw responseTooLarge(Number(declaredLength));
    }
    const body = await response.text();
    const responseBytes = new TextEncoder().encode(body).byteLength;
    if (responseBytes > MAX_LOADING_BAY_WEAPON_AUTHORING_RESPONSE_BYTES) {
      throw responseTooLarge(responseBytes);
    }
    if (!response.ok) {
      throw new LoadingBayWeaponTransportError(
        hostError(body, response.status),
      );
    }
    return body;
  }
}

export class LoadingBayWeaponAuthoringClient {
  private readonly port: LoadingBayWeaponAuthoringPort;
  private readonly nextRequestId: () => string;

  constructor(
    port: LoadingBayWeaponAuthoringPort,
    nextRequestId: () => string,
  ) {
    this.port = port;
    this.nextRequestId = nextRequestId;
  }

  async read(
    input: {
      readonly expectedProjectHash: string;
      readonly ownerEntityId: number;
    },
    signal: AbortSignal,
  ): Promise<LoadingBayWeaponReadout> {
    const requestId = this.nextRequestId();
    const response = await this.exchange(
      requestId,
      encodeReadLoadingBayWeaponRequest({ requestId, ...input }),
      signal,
    );
    if (response.type !== "loadingBayWeaponRead") {
      throw protocolError(
        "$.type",
        `expected loadingBayWeaponRead, received ${response.type}`,
      );
    }
    if (response.weapon.ownerEntityId !== input.ownerEntityId) {
      throw protocolError(
        "$.weapon.ownerEntityId",
        `expected owner ${String(input.ownerEntityId)}, received ${String(
          response.weapon.ownerEntityId,
        )}`,
      );
    }
    return response.weapon;
  }

  async replace(
    input: {
      readonly expectedProjectHash: string;
      readonly ownerEntityId: number;
      readonly expectedComponentRevision: string;
      readonly candidate: LoadingBayWeaponCandidate;
    },
    signal: AbortSignal,
  ): Promise<{
    readonly receipt: LoadingBayWeaponReceipt;
    readonly weapon: LoadingBayWeaponReadout;
  }> {
    const requestId = this.nextRequestId();
    const response = await this.exchange(
      requestId,
      encodeReplaceLoadingBayWeaponRequest({ requestId, ...input }),
      signal,
    );
    if (response.type !== "loadingBayWeaponReplaced") {
      throw protocolError(
        "$.type",
        `expected loadingBayWeaponReplaced, received ${response.type}`,
      );
    }
    requireReplacementCorrelation(response, input);
    return { receipt: response.receipt, weapon: response.weapon };
  }

  private async exchange(
    requestId: string,
    request: string,
    signal: AbortSignal,
  ): Promise<LoadingBayWeaponAuthoringResponse> {
    const response = decodeLoadingBayWeaponAuthoringResponse(
      await this.port.exchange(request, signal),
    );
    if (response.requestId !== requestId) {
      throw protocolError(
        "$.requestId",
        `expected ${JSON.stringify(requestId)}, received ${JSON.stringify(
          response.requestId,
        )}`,
      );
    }
    if (response.type === "loadingBayWeaponRejected") {
      throw new LoadingBayWeaponOperationRejected(
        response.requestId,
        response.rejection,
      );
    }
    return response;
  }
}

function requireReplacementCorrelation(
  response: Extract<
    LoadingBayWeaponAuthoringResponse,
    { readonly type: "loadingBayWeaponReplaced" }
  >,
  input: {
    readonly expectedProjectHash: string;
    readonly ownerEntityId: number;
    readonly expectedComponentRevision: string;
  },
): void {
  const { receipt, weapon } = response;
  if (
    receipt.ownerEntityId !== input.ownerEntityId ||
    weapon.ownerEntityId !== input.ownerEntityId
  ) {
    throw protocolError(
      "$.receipt.ownerEntityId",
      `replacement owner does not match requested owner ${String(
        input.ownerEntityId,
      )}`,
    );
  }
  if (receipt.projectHashBefore !== input.expectedProjectHash) {
    throw protocolError(
      "$.receipt.projectHashBefore",
      "replacement receipt does not begin at the requested project hash",
    );
  }
  if (receipt.componentRevisionBefore !== input.expectedComponentRevision) {
    throw protocolError(
      "$.receipt.componentRevisionBefore",
      "replacement receipt does not begin at the requested component revision",
    );
  }
  if (
    receipt.itemDefinitionId !== weapon.itemDefinitionId ||
    receipt.componentRevisionAfter !== weapon.componentRevision
  ) {
    throw protocolError(
      "$.weapon",
      "replacement weapon does not match its durable mutation receipt",
    );
  }
}

export function encodeReadLoadingBayWeaponRequest(input: {
  readonly requestId: string;
  readonly expectedProjectHash: string;
  readonly ownerEntityId: number;
}): string {
  return encodeRequest({
    type: "readLoadingBayWeapon",
    contractVersion: LOADING_BAY_WEAPON_AUTHORING_CONTRACT_VERSION,
    requestId: input.requestId,
    expectedProjectHash: input.expectedProjectHash,
    ownerEntityId: input.ownerEntityId,
  });
}

export function encodeReplaceLoadingBayWeaponRequest(input: {
  readonly requestId: string;
  readonly expectedProjectHash: string;
  readonly ownerEntityId: number;
  readonly expectedComponentRevision: string;
  readonly candidate: LoadingBayWeaponCandidate;
}): string {
  return encodeRequest({
    type: "replaceLoadingBayWeapon",
    contractVersion: LOADING_BAY_WEAPON_AUTHORING_CONTRACT_VERSION,
    requestId: input.requestId,
    expectedProjectHash: input.expectedProjectHash,
    ownerEntityId: input.ownerEntityId,
    expectedComponentRevision: input.expectedComponentRevision,
    candidate: input.candidate,
  });
}

export function decodeLoadingBayWeaponAuthoringResponse(
  input: string,
): LoadingBayWeaponAuthoringResponse {
  requireByteLimit(
    input,
    MAX_LOADING_BAY_WEAPON_AUTHORING_RESPONSE_BYTES,
    "response",
  );
  let decoded: unknown;
  try {
    decoded = JSON.parse(input);
  } catch (error) {
    throw protocolError("$", `invalid JSON: ${String(error)}`);
  }
  const envelope = record(decoded, "$");
  const type = stringField(envelope, "type", "$");
  switch (type) {
    case "loadingBayWeaponRead":
      exactKeys(envelope, "$", [
        "type",
        "contractVersion",
        "requestId",
        "weapon",
      ]);
      return {
        type,
        contractVersion: contractVersion(envelope, "$"),
        requestId: stringField(envelope, "requestId", "$"),
        weapon: weaponReadout(envelope.weapon, "$.weapon"),
      };
    case "loadingBayWeaponReplaced":
      exactKeys(envelope, "$", [
        "type",
        "contractVersion",
        "requestId",
        "receipt",
        "weapon",
      ]);
      return {
        type,
        contractVersion: contractVersion(envelope, "$"),
        requestId: stringField(envelope, "requestId", "$"),
        receipt: receipt(envelope.receipt, "$.receipt"),
        weapon: weaponReadout(envelope.weapon, "$.weapon"),
      };
    case "loadingBayWeaponRejected":
      exactKeys(envelope, "$", [
        "type",
        "contractVersion",
        "requestId",
        "rejection",
      ]);
      return {
        type,
        contractVersion: contractVersion(envelope, "$"),
        requestId: stringField(envelope, "requestId", "$"),
        rejection: rejection(envelope.rejection, "$.rejection"),
      };
    default:
      throw protocolError(
        "$.type",
        `unknown response type ${JSON.stringify(type)}`,
      );
  }
}

function encodeRequest(request: Readonly<Record<string, unknown>>): string {
  const encoded = JSON.stringify(request);
  requireByteLimit(
    encoded,
    MAX_LOADING_BAY_WEAPON_AUTHORING_REQUEST_BYTES,
    "request",
  );
  return encoded;
}

function weaponReadout(value: unknown, path: string): LoadingBayWeaponReadout {
  const input = record(value, path);
  exactKeys(input, path, [
    "componentTypeId",
    "contractId",
    "contractVersion",
    "ownerEntityId",
    "componentRevision",
    "itemDefinitionId",
    "binding",
    "definition",
  ]);
  const componentTypeId = stringField(input, "componentTypeId", path);
  if (componentTypeId !== LOADING_BAY_WEAPON_COMPONENT_TYPE_ID) {
    throw protocolError(
      `${path}.componentTypeId`,
      `unexpected component identity ${JSON.stringify(componentTypeId)}`,
    );
  }
  const contractId = stringField(input, "contractId", path);
  if (contractId !== LOADING_BAY_WEAPON_AUTHORING_CONTRACT_ID) {
    throw protocolError(
      `${path}.contractId`,
      `unexpected contract identity ${JSON.stringify(contractId)}`,
    );
  }
  return {
    componentTypeId,
    contractId,
    contractVersion: contractVersion(input, path),
    ownerEntityId: safeIntegerField(input, "ownerEntityId", path),
    componentRevision: hashField(input, "componentRevision", path),
    itemDefinitionId: stringField(input, "itemDefinitionId", path),
    binding: binding(input.binding, `${path}.binding`),
    definition: candidate(input.definition, `${path}.definition`),
  };
}

function binding(value: unknown, path: string): LoadingBayWeaponBinding {
  const input = record(value, path);
  exactKeys(input, path, [
    "inventoryOwnerEntityId",
    "slotIndex",
    "startingQuantity",
    "initiallyEquipped",
  ]);
  return {
    inventoryOwnerEntityId: safeIntegerField(
      input,
      "inventoryOwnerEntityId",
      path,
    ),
    slotIndex: safeIntegerField(input, "slotIndex", path),
    startingQuantity: safeIntegerField(input, "startingQuantity", path),
    initiallyEquipped: booleanField(input, "initiallyEquipped", path),
  };
}

function candidate(value: unknown, path: string): LoadingBayWeaponCandidate {
  const input = record(value, path);
  exactKeys(input, path, [
    "attackMode",
    "damage",
    "maxDistance",
    "cooldownTicks",
    "ammunitionItemId",
    "ammunitionCost",
    "muzzleOffset",
    "presentation",
  ]);
  return {
    attackMode: attackMode(input.attackMode, `${path}.attackMode`),
    damage: safeIntegerField(input, "damage", path),
    maxDistance: finiteNumberField(input, "maxDistance", path),
    cooldownTicks: safeIntegerField(input, "cooldownTicks", path),
    ammunitionItemId: stringField(input, "ammunitionItemId", path),
    ammunitionCost: safeIntegerField(input, "ammunitionCost", path),
    muzzleOffset: vector(input.muzzleOffset, `${path}.muzzleOffset`),
    presentation: stringField(input, "presentation", path),
  };
}

function attackMode(value: unknown, path: string): LoadingBayWeaponAttackMode {
  const input = record(value, path);
  const mode = stringField(input, "mode", path);
  switch (mode) {
    case "hitscan":
    case "automatic":
      exactKeys(input, path, ["mode"]);
      return { mode };
    case "spread":
      exactKeys(input, path, ["mode", "pelletCount", "spreadDegrees"]);
      return {
        mode,
        pelletCount: safeIntegerField(input, "pelletCount", path),
        spreadDegrees: finiteNumberField(input, "spreadDegrees", path),
      };
    default:
      throw protocolError(
        `${path}.mode`,
        `unknown attack mode ${JSON.stringify(mode)}`,
      );
  }
}

function receipt(value: unknown, path: string): LoadingBayWeaponReceipt {
  const input = record(value, path);
  exactKeys(input, path, [
    "ownerEntityId",
    "itemDefinitionId",
    "projectHashBefore",
    "projectHashAfter",
    "componentRevisionBefore",
    "componentRevisionAfter",
  ]);
  return {
    ownerEntityId: safeIntegerField(input, "ownerEntityId", path),
    itemDefinitionId: stringField(input, "itemDefinitionId", path),
    projectHashBefore: hashField(input, "projectHashBefore", path),
    projectHashAfter: hashField(input, "projectHashAfter", path),
    componentRevisionBefore: hashField(input, "componentRevisionBefore", path),
    componentRevisionAfter: hashField(input, "componentRevisionAfter", path),
  };
}

const REJECTION_CODES = new Set<LoadingBayWeaponRejectionCode>([
  "unsupportedContractVersion",
  "invalidRequestId",
  "invalidProjectHash",
  "staleProject",
  "projectRejected",
  "weaponNotFound",
  "staleComponent",
  "candidateRejected",
  "projectStoreFailure",
]);

function rejection(value: unknown, path: string): LoadingBayWeaponRejection {
  const input = record(value, path);
  exactKeys(input, path, ["code", "message", "path"], ["path"]);
  const code = stringField(input, "code", path);
  if (!REJECTION_CODES.has(code as LoadingBayWeaponRejectionCode)) {
    throw protocolError(
      `${path}.code`,
      `unknown rejection code ${JSON.stringify(code)}`,
    );
  }
  const sourcePath =
    input.path === undefined ? undefined : stringField(input, "path", path);
  return {
    code: code as LoadingBayWeaponRejectionCode,
    message: stringField(input, "message", path),
    ...(sourcePath === undefined ? {} : { path: sourcePath }),
  };
}

function contractVersion(
  input: Readonly<Record<string, unknown>>,
  path: string,
): 1 {
  const version = safeIntegerField(input, "contractVersion", path);
  if (version !== LOADING_BAY_WEAPON_AUTHORING_CONTRACT_VERSION) {
    throw protocolError(
      `${path}.contractVersion`,
      `unsupported contract version ${version}`,
    );
  }
  return version;
}

function vector(
  value: unknown,
  path: string,
): readonly [number, number, number] {
  if (!Array.isArray(value) || value.length !== 3) {
    throw protocolError(path, "expected a three-number vector");
  }
  return [
    finiteNumber(value[0], `${path}[0]`),
    finiteNumber(value[1], `${path}[1]`),
    finiteNumber(value[2], `${path}[2]`),
  ];
}

function record(value: unknown, path: string): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw protocolError(path, "expected an object");
  }
  return value as Record<string, unknown>;
}

function exactKeys(
  input: Readonly<Record<string, unknown>>,
  path: string,
  keys: readonly string[],
  optional: readonly string[] = [],
): void {
  const allowed = new Set(keys);
  for (const key of Object.keys(input)) {
    if (!allowed.has(key)) {
      throw protocolError(`${path}.${key}`, "unknown field");
    }
  }
  const optionalKeys = new Set(optional);
  for (const key of keys) {
    if (!optionalKeys.has(key) && !(key in input)) {
      throw protocolError(`${path}.${key}`, "missing field");
    }
  }
}

function stringField(
  input: Readonly<Record<string, unknown>>,
  key: string,
  path: string,
): string {
  const value = input[key];
  if (typeof value !== "string") {
    throw protocolError(`${path}.${key}`, "expected a string");
  }
  return value;
}

function hashField(
  input: Readonly<Record<string, unknown>>,
  key: string,
  path: string,
): string {
  const value = stringField(input, key, path);
  if (!/^[0-9a-f]{64}$/u.test(value)) {
    throw protocolError(`${path}.${key}`, "expected a lowercase content hash");
  }
  return value;
}

function booleanField(
  input: Readonly<Record<string, unknown>>,
  key: string,
  path: string,
): boolean {
  const value = input[key];
  if (typeof value !== "boolean") {
    throw protocolError(`${path}.${key}`, "expected a boolean");
  }
  return value;
}

function safeIntegerField(
  input: Readonly<Record<string, unknown>>,
  key: string,
  path: string,
): number {
  const value = input[key];
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0) {
    throw protocolError(
      `${path}.${key}`,
      "expected a non-negative safe integer",
    );
  }
  return value;
}

function finiteNumberField(
  input: Readonly<Record<string, unknown>>,
  key: string,
  path: string,
): number {
  return finiteNumber(input[key], `${path}.${key}`);
}

function finiteNumber(value: unknown, path: string): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw protocolError(path, "expected a finite number");
  }
  return value;
}

function requireByteLimit(input: string, limit: number, label: string): void {
  const actual = new TextEncoder().encode(input).byteLength;
  if (actual > limit) {
    throw protocolError(
      "$",
      `${label} is ${actual} bytes, exceeding the ${limit}-byte bound`,
    );
  }
}

function protocolError(
  path: string,
  message: string,
): LoadingBayWeaponProtocolError {
  return new LoadingBayWeaponProtocolError(`${message} at ${path}`);
}

function responseTooLarge(actual: number): LoadingBayWeaponTransportError {
  return new LoadingBayWeaponTransportError(
    `weapon authoring response is ${String(actual)} bytes, exceeding the ${String(
      MAX_LOADING_BAY_WEAPON_AUTHORING_RESPONSE_BYTES,
    )}-byte bound`,
  );
}

function hostError(body: string, status: number): string {
  try {
    const decoded = JSON.parse(body) as unknown;
    if (
      decoded !== null &&
      typeof decoded === "object" &&
      !Array.isArray(decoded)
    ) {
      const message = (decoded as Record<string, unknown>).message;
      if (typeof message === "string" && message.length > 0) return message;
    }
  } catch {
    // Preserve the stable HTTP fallback for non-JSON host failures.
  }
  return `Studio host rejected weapon authoring with HTTP ${String(status)}`;
}

import { invoke, isTauri } from "@tauri-apps/api/core";
import {
  createRustyDeveloperCommandClient,
  RUSTY_STANDARD_HOST_WIRE_SCHEMAS,
  type RustyDeveloperCommandAdapter,
  type RustyDeveloperCommandClient,
  type RustyDeveloperCommandExtension,
  type RustyDeveloperCommandValueSchema,
  type RustyDeveloperCommandWireSchema,
} from "@rusty-engine/application-host";

const DEVELOPER_PROTOCOL = "loading-bay.developer-command.v1";
const POLL_INTERVAL_MS = 8;
const POLL_TIMEOUT_MS = 10_000;

const opaque: RustyDeveloperCommandValueSchema = {
  kind: "opaqueJson",
  maximumBytes: 32_768,
  maximumNodes: 512,
};
const playSchema: RustyDeveloperCommandWireSchema = {
  request: opaque,
  result: {
    kind: "object",
    fields: {
      kind: { required: true, value: { kind: "string", maximumBytes: 96 } },
      connectionGeneration: {
        required: true,
        value: { kind: "integer", minimum: 0 },
      },
      commandSequence: {
        required: true,
        value: { kind: "integer", minimum: 0 },
      },
    },
  },
  error: opaque,
};

/**
 * This attaches a codec to the command the Rust discovery already exposes.
 * It deliberately contributes no descriptor or executable route.
 */
export const LOADING_BAY_DEVELOPER_COMMAND_EXTENSION: RustyDeveloperCommandExtension =
  {
    namespace: "loading-bay",
    schemas: [
      {
        command: "loading-bay.play.service-command",
        lane: "play",
        profile: "loading-bay.developer",
        schema: playSchema,
      },
    ],
  };

export function createLoadingBayDeveloperCommandClient(): RustyDeveloperCommandClient {
  return createRustyDeveloperCommandClient({
    adapter: isTauri() ? tauriAdapter : browserAdapter,
    schemas: RUSTY_STANDARD_HOST_WIRE_SCHEMAS,
    extensions: [LOADING_BAY_DEVELOPER_COMMAND_EXTENSION],
  });
}

const browserAdapter: RustyDeveloperCommandAdapter = {
  discover: (signal) => socketRequest({ kind: "discover" }, signal),
  execute: (request, signal) =>
    socketRequest({ kind: "execute", request }, signal, {
      kind: "cancel",
      correlation: request.correlation,
    }),
};

const tauriAdapter: RustyDeveloperCommandAdapter = {
  discover: (signal) =>
    invokeWithSignal("loading_bay_developer_discover", undefined, signal),
  execute: async (request, signal) => {
    throwIfAborted(signal);
    await invoke("loading_bay_developer_submit", { request });
    const deadline = performance.now() + POLL_TIMEOUT_MS;
    while (performance.now() < deadline) {
      if (signal?.aborted === true) {
        await invoke("loading_bay_developer_cancel", {
          correlation: request.correlation,
        }).catch(() => undefined);
        throw (
          signal.reason ??
          new DOMException("Developer command cancelled", "AbortError")
        );
      }
      const response = await invoke<unknown | null>(
        "loading_bay_developer_poll",
        {
          correlation: request.correlation,
        },
      );
      if (response !== null) return response;
      await delay(POLL_INTERVAL_MS);
    }
    await invoke("loading_bay_developer_cancel", {
      correlation: request.correlation,
    }).catch(() => undefined);
    throw new Error("developer command timed out");
  },
};

function socketRequest(
  payload: Readonly<Record<string, unknown>>,
  signal?: AbortSignal,
  cancellation?: Readonly<Record<string, unknown>>,
): Promise<unknown> {
  throwIfAborted(signal);
  return new Promise<unknown>((resolve, reject) => {
    const socket = new WebSocket(developerSocketUrl(), DEVELOPER_PROTOCOL);
    let settled = false;
    const finish = (action: () => void): void => {
      if (settled) return;
      settled = true;
      signal?.removeEventListener("abort", abort);
      socket.close();
      action();
    };
    const abort = (): void => {
      if (cancellation !== undefined && socket.readyState === WebSocket.OPEN) {
        socket.send(JSON.stringify(cancellation));
      }
      finish(() =>
        reject(
          signal?.reason ??
            new DOMException("Developer command cancelled", "AbortError"),
        ),
      );
    };
    signal?.addEventListener("abort", abort, { once: true });
    socket.addEventListener(
      "open",
      () => socket.send(JSON.stringify(payload)),
      { once: true },
    );
    socket.addEventListener("message", (event) => {
      try {
        const response: unknown = JSON.parse(String(event.data));
        if (
          !isRecord(response) ||
          (response.kind !== "success" && response.kind !== "error")
        ) {
          throw new Error(
            "developer-command host returned a malformed response",
          );
        }
        if (response.kind === "error") {
          throw new Error(
            typeof response.message === "string"
              ? response.message
              : "developer command failed",
          );
        }
        finish(() => resolve(response.value));
      } catch (cause) {
        finish(() => reject(cause));
      }
    });
    socket.addEventListener(
      "error",
      () => {
        finish(() =>
          reject(new Error("developer-command WebSocket is unavailable")),
        );
      },
      { once: true },
    );
    socket.addEventListener(
      "close",
      () => {
        finish(() =>
          reject(
            new Error("developer-command WebSocket closed before a response"),
          ),
        );
      },
      { once: true },
    );
  });
}

async function invokeWithSignal(
  command: string,
  args: Record<string, unknown> | undefined,
  signal?: AbortSignal,
): Promise<unknown> {
  throwIfAborted(signal);
  const result = await invoke(command, args);
  throwIfAborted(signal);
  return result;
}

function developerSocketUrl(): string {
  const protocol = location.protocol === "https:" ? "wss:" : "ws:";
  return `${protocol}//${location.host}/api/developer-command`;
}

function throwIfAborted(signal?: AbortSignal): void {
  if (signal?.aborted === true) {
    throw (
      signal.reason ??
      new DOMException("Developer command cancelled", "AbortError")
    );
  }
}

function delay(milliseconds: number): Promise<void> {
  return new Promise((resolve) => globalThis.setTimeout(resolve, milliseconds));
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

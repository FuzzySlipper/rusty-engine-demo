# Loading Bay browser session protocol

`loading-bay.v2` is the bounded WebSocket protocol used by `browser-host`, the lightweight development adapter over `LoadingBayProductService`. It is not a generic Engine bridge.

## Session

The client opens `/api/session`, receives a typed bootstrap/projection, and obtains a connection generation. Every input or edge command carries that generation plus a monotonic sequence. Rust rejects stale, retired, malformed, or out-of-bounds commands before they enter gameplay. Disconnect retires the session generation.

Continuous input is latest-wins (`movement`, `lookDelta`, held jump/fire). Discrete actions are closed semantic edges: interaction, item use, weapon selection, pause, restart, save, and load. The browser may coalesce continuous input but must preserve accepted edge ordering. Rust owns admission, fixed-step consumption, acknowledgements, facts, and full/delta projection.

`GET /health` and `GET /api/state` are read-only diagnostics. There are no HTTP gameplay mutators.

## Limits and ownership

The wire shape is deliberately bounded; client and server enforce command, queue, and update limits. TypeScript treats accepted projection as immutable presentation input and never derives gameplay state locally. Static render resources are supplied through the admitted Rust projection and mounted only through the public Engine application-host.

Tauri does not tunnel this WebSocket protocol: it calls the same Rust service through typed in-process IPC. The service contract, not an unbounded JSON/RPC method dispatcher, is the shared product seam.

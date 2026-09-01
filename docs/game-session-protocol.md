# Loading Bay browser integration

The browser integration is the public Rusty Engine Product Browser Host. It mounts the NativeAOT `LoadingBayProduct`, owns realtime lifecycle/cadence and renderer/canvas lifetime, drains bounded semantic input, and subscribes to product UI projection.

Loading Bay configures the host with a closed set of E1M1 semantic intents: movement, jump, use, fire, and look axes. The Angular shell may capture and normalize browser events within host bounds, but the C# product and Engine decide how admitted input affects state. The browser cannot send arbitrary product methods, mutate state through HTTP, select a renderer resource, or become a second timing authority.

The read-only UI contract is `loading-bay.hud.snapshot.v1` on `loading-bay.hud`. Its HUD data is copied from the C# product projection and includes only bounded presentation/readout fields. Keep a contract change explicit in C#, shell validation, and focused browser proof.

The host owns transport details behind its public package. Downstream code
consumes the typed product/UI surface and does not duplicate host plumbing.

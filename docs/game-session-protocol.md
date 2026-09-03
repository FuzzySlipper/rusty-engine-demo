# Loading Bay browser integration

The matched runtime pack hosts the staged CoreCLR `LoadingBayProduct`. It owns realtime lifecycle/cadence, renderer preload and canvas lifetime, bounded semantic input, and the product UI projection channel.

`LoadingBay.Game` declares the closed E1M1 semantic intent set: movement, jump, use, fire, and look axes. The runtime pack owns physical-event capture and transport. The C# product and Engine decide how admitted input affects state. The browser cannot send arbitrary product methods, mutate state through HTTP, select a renderer resource, or become a second timing authority.

The Angular entry exports `mountProductUi`. The runtime shell calls it with the read-only UI context after its host is ready. The contract is `loading-bay.hud.snapshot.v1` on `loading-bay.hud`; its HUD data is copied from the C# product projection and includes only bounded presentation/readout fields. Keep a contract change explicit in C#, shell validation, and focused browser proof.

The staged Angular output keeps `main.js` as its declared module entry. It is product UI only; the runtime shell remains the owner of browser bootstrap and transport details.

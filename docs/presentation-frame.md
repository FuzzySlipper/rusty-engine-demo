# Presentation-frame contract

All rendering, loading, failure, progress indicators, and interactive UI of a
Loading Bay session live inside **one bounded viewport**. The contract has
three rules:

1. **Everything visible is in the frame.** The game view, loading and failure
   states, HUD, prompts, developer-console affordances, and error indicators
   render inside the same bounded surface. No control lives off-canvas, in
   scrolling document overflow, or behind an unrelated page region.
2. **Gutters reject input.** Coordinates outside the viewport bounds never
   reach the semantic-input path. Look input is captured through pointer lock
   on the viewport element itself (`ts/packages/browser-shell/src/game-runtime.ts`),
   so page-level cursor positions never become camera intent; a click in the
   letterbox gutter cannot fire, move the camera, or select a menu item.
3. **The frame owns its own states.** Resize, loading, and failure re-layout
   within the viewport; the application never pushes the player to scroll or
   resize their window to reach a control or read an indicator.

## Why this makes browser proof transferable

The browser adapter renders the same Angular shell through the same public
`@rusty-engine/application-host` surface that the Tauri WebView uses. Because
every claimable behavior is confined to one bounded frame:

- a deterministic browser screenshot at viewport size shows everything a
  reviewer needs — there is no second window, hidden pane, or scroll position
  that could differ;
- resizing behaves identically in a browser window and a native game window,
  because both consume the same shell layout inside the same frame;
- Tauri packaging cannot hide a regression: if it renders in the bounded
  browser frame, the identical shell renders in the WebView.

Headed evidence is therefore required only for claims about the visible
viewport/console itself (per the verification policy); ordinary gameplay and
service proofs stay deterministic.

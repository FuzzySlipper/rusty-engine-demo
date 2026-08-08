import {
  ChangeDetectionStrategy,
  Component,
  afterNextRender,
} from "@angular/core";
import { RouterLink } from "@angular/router";

@Component({
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink],
  selector: "red-diagnostics-screen",
  standalone: true,
  template: `
    <main class="lifecycle-screen">
      <p class="eyebrow">Projection lifecycle proof</p>
      <h1>Browser projection released</h1>
      <p>
        This route owns no renderer. Returning to the game reconnects the
        HUD/control projection while the rendered world remains in the native
        Engine host.
      </p>
      <a routerLink="/game">Return to Loading Bay</a>
    </main>
  `,
})
export class DiagnosticsScreenComponent {
  constructor() {
    afterNextRender(() => {
      const disposed =
        document.querySelector("red-game-screen") === null &&
        document.querySelector("#viewport") === null;
      document.body.dataset.rendererLifecycle = "disposed";
      document.body.dataset.routeDisposal = disposed ? "pass" : "fail";
      if (new URLSearchParams(location.search).has("lifecycle-smoke")) {
        document.body.dataset.smokeStatus = disposed ? "pass" : "fail";
      }
    });
  }
}

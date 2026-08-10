import {
  ChangeDetectionStrategy,
  Component,
  afterNextRender,
  inject,
} from "@angular/core";
import { RouterLink } from "@angular/router";
import { ENGINE_APPLICATION } from "./engine-application";

@Component({
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink],
  selector: "red-diagnostics-screen",
  standalone: true,
  template: `
    <main class="lifecycle-screen">
      <p class="eyebrow">Projection lifecycle proof</p>
      <h1>Game projection released</h1>
      <p>
        The game route and its Rust session are disposed. The Engine application
        host remains mounted once for the product shell with an empty retained
        frame; returning to the game reconnects Rust facts and the rich UI.
      </p>
      <a routerLink="/game">Return to Loading Bay</a>
    </main>
  `,
})
export class DiagnosticsScreenComponent {
  private readonly engineApplication = inject(ENGINE_APPLICATION);

  constructor() {
    afterNextRender(() => {
      void this.proveReleasedGameRoute();
    });
  }

  private async proveReleasedGameRoute(): Promise<void> {
    this.engineApplication.ui.setInteractionMode("interface");
    await this.engineApplication.renderer.clear();
    const disposed =
      document.querySelector("red-game-screen") === null &&
      document.querySelector("#viewport") === null &&
      document.querySelectorAll(
        "canvas[data-rusty-application-renderer='engine-owned']",
      ).length === 1;
    document.body.dataset.rendererLifecycle = "application-host-idle";
    document.body.dataset.routeDisposal = disposed ? "pass" : "fail";
    if (new URLSearchParams(location.search).has("lifecycle-smoke")) {
      document.body.dataset.smokeStatus = disposed ? "pass" : "fail";
    }
  }
}

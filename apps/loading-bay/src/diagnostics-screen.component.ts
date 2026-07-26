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
      <p class="eyebrow">Renderer lifecycle proof</p>
      <h1>Shared surface released</h1>
      <p>
        This route owns no renderer. Returning to the game creates one new
        shared surface under the game route lifecycle.
      </p>
      <a routerLink="/">Return to Loading Bay</a>
    </main>
  `,
})
export class DiagnosticsScreenComponent {
  constructor() {
    afterNextRender(() => {
      const disposed = document.body.dataset.rendererLifecycle === "disposed";
      document.body.dataset.routeDisposal = disposed ? "pass" : "fail";
      if (new URLSearchParams(location.search).has("lifecycle-smoke")) {
        document.body.dataset.smokeStatus = disposed ? "pass" : "fail";
      }
    });
  }
}

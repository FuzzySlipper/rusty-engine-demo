import { ChangeDetectionStrategy, Component } from "@angular/core";
import { LoadingBayHudComponent } from "./loading-bay-hud.component";

@Component({
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [LoadingBayHudComponent],
  selector: "red-root",
  standalone: true,
  styles: [
    `
      :host {
        display: block;
        min-height: 100vh;
      }
    `,
  ],
  template: `<red-loading-bay-hud />`,
})
export class AppComponent {}

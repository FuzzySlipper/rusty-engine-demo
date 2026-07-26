import { ChangeDetectionStrategy, Component, input } from "@angular/core";

const CARDINALS: readonly {
  readonly bearing: number;
  readonly label: string;
}[] = [
  { bearing: 0, label: "N" },
  { bearing: 90, label: "E" },
  { bearing: 180, label: "S" },
  { bearing: 270, label: "W" },
];

@Component({
  changeDetection: ChangeDetectionStrategy.OnPush,
  selector: "red-compass",
  standalone: true,
  styles: [
    `
      :host {
        display: block;
        width: min(300px, 55vw);
      }
      .strip {
        background: var(--rusty-engine-surface);
        border-block: 1px solid var(--rusty-engine-border);
        height: 30px;
        overflow: hidden;
        position: relative;
      }
      .tick {
        color: var(--rusty-engine-muted);
        font:
          700 0.7rem "SFMono-Regular",
          Consolas,
          monospace;
        position: absolute;
        top: 50%;
        transform: translate(-50%, -50%);
      }
      .center {
        background: var(--rusty-engine-accent);
        box-shadow: 0 0 8px var(--rusty-engine-accent);
        height: 100%;
        left: 50%;
        position: absolute;
        top: 0;
        width: 1px;
      }
    `,
  ],
  template: `
    <div
      class="strip"
      role="img"
      [attr.aria-label]="'Heading ' + roundedHeading() + ' degrees'"
    >
      <span class="center"></span>
      @for (tick of cardinalTicks; track tick.label) {
        @if (isVisible(tick.bearing)) {
          <span class="tick" [style.left.%]="offsetPercent(tick.bearing)">
            {{ tick.label }}
          </span>
        }
      }
    </div>
  `,
})
export class CompassComponent {
  readonly headingDegrees = input.required<number>();
  protected readonly cardinalTicks = CARDINALS;

  protected roundedHeading(): number {
    return Math.round(((this.headingDegrees() % 360) + 360) % 360);
  }

  protected isVisible(bearing: number): boolean {
    return Math.abs(this.relativeBearing(bearing)) <= 75;
  }

  protected offsetPercent(bearing: number): number {
    return 50 + (this.relativeBearing(bearing) / 75) * 50;
  }

  private relativeBearing(bearing: number): number {
    return ((((bearing - this.headingDegrees()) % 360) + 540) % 360) - 180;
  }
}

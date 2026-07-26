import { ChangeDetectionStrategy, Component, input } from "@angular/core";

export interface CombatLogEntryView {
  readonly id: number;
  readonly source: string;
  readonly text: string;
  readonly severity: "hit" | "info" | "miss" | "system";
}

@Component({
  changeDetection: ChangeDetectionStrategy.OnPush,
  selector: "red-combat-log",
  standalone: true,
  styles: [
    `
      :host {
        display: block;
        width: min(320px, calc(100vw - 124px));
      }
      .entries {
        display: flex;
        flex-direction: column;
        gap: 2px;
        list-style: none;
        margin: 0;
        max-height: 118px;
        overflow: hidden;
        padding: 0;
      }
      .entry {
        font-family: "SFMono-Regular", Consolas, monospace;
        font-size: 0.7rem;
        line-height: 1.35;
        text-shadow: 0 1px 3px #000;
      }
      .entry__source {
        font-weight: 700;
        margin-right: 0.35rem;
      }
      .entry--info {
        color: var(--rusty-engine-muted);
      }
      .entry--hit {
        color: var(--rusty-engine-accent);
      }
      .entry--miss {
        color: var(--rusty-engine-danger);
      }
      .entry--system {
        color: var(--rusty-engine-warn);
      }
      .empty {
        color: var(--rusty-engine-muted);
        font-size: 0.72rem;
        margin: 0;
      }
    `,
  ],
  template: `
    <section class="rusty-engine-panel" aria-label="Committed game facts">
      <h2 class="rusty-engine-panel__title">Committed facts</h2>
      @if (entries().length === 0) {
        <p class="empty">Awaiting action</p>
      } @else {
        <ul class="entries">
          @for (entry of entries(); track entry.id) {
            <li class="entry" [class]="'entry entry--' + entry.severity">
              <span class="entry__source">[{{ entry.source }}]</span
              >{{ entry.text }}
            </li>
          }
        </ul>
      }
    </section>
  `,
})
export class CombatLogComponent {
  readonly entries = input.required<readonly CombatLogEntryView[]>();
}

import {
  ChangeDetectionStrategy,
  Component,
  input,
  output,
} from "@angular/core";

export interface GameHotbarSlotView {
  readonly slot: number;
  readonly keybind: string;
  readonly label: string;
  readonly owned: boolean;
  readonly selected: boolean;
  readonly ammunition: number;
}

@Component({
  changeDetection: ChangeDetectionStrategy.OnPush,
  selector: "red-game-hotbar",
  standalone: true,
  styles: [
    `
      :host {
        display: block;
        pointer-events: none;
      }
      .slots {
        display: flex;
        gap: 0.45rem;
        list-style: none;
        margin: 0;
        padding: 0;
      }
      button {
        background: rgb(4 14 16 / 82%);
        border: 1px solid var(--rusty-engine-border);
        color: var(--rusty-engine-text);
        cursor: pointer;
        display: grid;
        gap: 0.1rem;
        min-height: 3.5rem;
        min-width: 7.2rem;
        padding: 0.45rem 0.65rem;
        pointer-events: auto;
        text-align: left;
      }
      button:hover:not(:disabled),
      button:focus-visible {
        border-color: var(--rusty-engine-accent);
        outline: none;
      }
      button[aria-pressed="true"] {
        background: rgb(30 84 71 / 88%);
        box-shadow: 0 0 0 1px var(--rusty-engine-accent);
      }
      button:disabled {
        cursor: not-allowed;
        opacity: 0.42;
      }
      .key {
        color: var(--rusty-engine-warn);
        font-size: 0.62rem;
        letter-spacing: 0.12em;
      }
      strong {
        font-size: 0.72rem;
        letter-spacing: 0.04em;
      }
      small {
        color: var(--rusty-engine-muted);
        font-size: 0.62rem;
      }
      @media (max-width: 680px) {
        button {
          min-width: 4.25rem;
          padding-inline: 0.4rem;
        }
        strong {
          max-width: 4rem;
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
        }
      }
    `,
  ],
  template: `
    <nav aria-label="Owned weapons">
      <ul class="slots">
        @for (slot of slots(); track slot.slot) {
          <li>
            <button
              type="button"
              [disabled]="disabled() || !slot.owned"
              [attr.aria-pressed]="slot.selected"
              [attr.aria-label]="
                slot.owned
                  ? slot.keybind +
                    ' ' +
                    slot.label +
                    ', ' +
                    slot.ammunition +
                    ' ammunition'
                  : slot.keybind + ' ' + slot.label + ', not owned'
              "
              (click)="weaponSelected.emit(slot.slot)"
            >
              <span class="key">{{ slot.keybind }}</span>
              <strong>{{ slot.label }}</strong>
              <small>{{
                slot.owned ? slot.ammunition + " AMMO" : "NOT OWNED"
              }}</small>
            </button>
          </li>
        }
      </ul>
    </nav>
  `,
})
export class GameHotbarComponent {
  readonly slots = input.required<readonly GameHotbarSlotView[]>();
  readonly disabled = input(false);
  readonly weaponSelected = output<number>();
}

export interface InventoryStackView {
  readonly item: string;
  readonly label: string;
  readonly quantity: number;
  readonly category: "ammunition" | "armor" | "key" | "supply" | "weapon";
  readonly usable: boolean;
}

export interface InventoryWeaponView {
  readonly slot: number;
  readonly label: string;
  readonly owned: boolean;
  readonly selected: boolean;
  readonly ammunitionLabel: string;
  readonly ammunitionQuantity: number;
}

@Component({
  changeDetection: ChangeDetectionStrategy.OnPush,
  selector: "red-inventory-panel",
  standalone: true,
  styles: [
    `
      :host {
        display: block;
      }
      .summary {
        color: var(--rusty-engine-muted);
        display: flex;
        font-size: 0.7rem;
        justify-content: space-between;
        margin-block: 0 1rem;
      }
      .grid {
        display: grid;
        gap: 1rem;
        grid-template-columns: minmax(0, 1.35fr) minmax(14rem, 1fr);
      }
      h3 {
        color: var(--rusty-engine-accent);
        font-size: 0.7rem;
        letter-spacing: 0.12em;
        margin: 0 0 0.55rem;
        text-transform: uppercase;
      }
      ul {
        display: grid;
        gap: 0.4rem;
        list-style: none;
        margin: 0;
        padding: 0;
      }
      .stack,
      .weapon {
        align-items: center;
        background: rgb(4 14 16 / 76%);
        border: 1px solid var(--rusty-engine-border);
        display: grid;
        gap: 0.25rem 0.65rem;
        grid-template-columns: minmax(0, 1fr) auto;
        min-height: 3rem;
        padding: 0.55rem 0.65rem;
      }
      .weapon--selected {
        border-color: var(--rusty-engine-accent);
      }
      strong {
        overflow-wrap: anywhere;
      }
      small,
      .empty {
        color: var(--rusty-engine-muted);
        font-size: 0.68rem;
      }
      .quantity {
        color: var(--rusty-engine-warn);
        font-family: "SFMono-Regular", Consolas, monospace;
      }
      button {
        background: transparent;
        border: 1px solid var(--rusty-engine-border);
        color: var(--rusty-engine-accent);
        cursor: pointer;
        font-size: 0.66rem;
        padding: 0.35rem 0.55rem;
      }
      button:disabled {
        color: var(--rusty-engine-muted);
        cursor: not-allowed;
        opacity: 0.5;
      }
      @media (max-width: 700px) {
        .grid {
          grid-template-columns: minmax(0, 1fr);
        }
      }
    `,
  ],
  template: `
    <section aria-label="Authoritative inventory">
      <p class="summary">
        <span>Rust-owned item stacks</span>
        <span>{{ stacks().length }} / {{ capacitySlots() }} slots</span>
      </p>
      <div class="grid">
        <div>
          <h3>Carried items</h3>
          @if (stacks().length === 0) {
            <p class="empty">No items are currently carried.</p>
          } @else {
            <ul>
              @for (stack of stacks(); track stack.item) {
                <li class="stack">
                  <span>
                    <strong>{{ stack.label }}</strong>
                    <small>{{ stack.category }}</small>
                  </span>
                  @if (stack.usable) {
                    <button
                      type="button"
                      [disabled]="busy()"
                      (click)="itemUsed.emit(stack.item)"
                    >
                      USE ×{{ stack.quantity }}
                    </button>
                  } @else {
                    <span class="quantity">×{{ stack.quantity }}</span>
                  }
                </li>
              }
            </ul>
          }
        </div>
        <div>
          <h3>Weapon slots</h3>
          <ul>
            @for (weapon of weapons(); track weapon.slot) {
              <li class="weapon" [class.weapon--selected]="weapon.selected">
                <span>
                  <strong>{{ weapon.slot + 1 }} · {{ weapon.label }}</strong>
                  <small
                    >{{ weapon.ammunitionQuantity }}
                    {{ weapon.ammunitionLabel }}</small
                  >
                </span>
                <button
                  type="button"
                  [disabled]="busy() || !weapon.owned || weapon.selected"
                  (click)="weaponSelected.emit(weapon.slot)"
                >
                  {{
                    weapon.selected
                      ? "EQUIPPED"
                      : weapon.owned
                        ? "EQUIP"
                        : "NOT OWNED"
                  }}
                </button>
              </li>
            }
          </ul>
        </div>
      </div>
    </section>
  `,
})
export class InventoryPanelComponent {
  readonly stacks = input.required<readonly InventoryStackView[]>();
  readonly weapons = input.required<readonly InventoryWeaponView[]>();
  readonly capacitySlots = input.required<number>();
  readonly busy = input(false);
  readonly itemUsed = output<string>();
  readonly weaponSelected = output<number>();
}

export interface HostSettingsView {
  readonly mouseSensitivity: number;
  readonly invertY: boolean;
  readonly sfxVolume: number;
  readonly flashIntensity: number;
  readonly hudVisible: boolean;
  readonly telemetryVisible: boolean;
}

export interface KeyBindingView {
  readonly action: string;
  readonly binding: string;
}

@Component({
  changeDetection: ChangeDetectionStrategy.OnPush,
  selector: "red-settings-panel",
  standalone: true,
  styles: [
    `
      :host {
        display: block;
      }
      .settings {
        display: grid;
        gap: 0.85rem;
      }
      label {
        align-items: center;
        background: rgb(4 14 16 / 65%);
        border: 1px solid var(--rusty-engine-border);
        display: grid;
        gap: 0.35rem;
        grid-template-columns: minmax(10rem, 1fr) minmax(9rem, 1fr) auto;
        padding: 0.7rem;
      }
      label span,
      h3 {
        font-size: 0.7rem;
        letter-spacing: 0.1em;
        text-transform: uppercase;
      }
      output {
        color: var(--rusty-engine-warn);
        font-family: "SFMono-Regular", Consolas, monospace;
        min-width: 3rem;
        text-align: right;
      }
      .toggle {
        grid-template-columns: minmax(0, 1fr) auto;
      }
      input[type="checkbox"] {
        height: 1.1rem;
        width: 1.1rem;
      }
      h3 {
        color: var(--rusty-engine-accent);
        margin: 0.5rem 0 0;
      }
      dl {
        display: grid;
        gap: 0.25rem;
        grid-template-columns: minmax(8rem, 1fr) minmax(8rem, 1fr);
        margin: 0;
      }
      dt,
      dd {
        border-bottom: 1px solid var(--rusty-engine-border);
        margin: 0;
        padding: 0.35rem 0;
      }
      dd {
        color: var(--rusty-engine-warn);
        font-family: "SFMono-Regular", Consolas, monospace;
        text-align: right;
      }
      .note {
        color: var(--rusty-engine-muted);
        font-size: 0.68rem;
        line-height: 1.45;
        margin: 0;
      }
      @media (max-width: 600px) {
        label {
          grid-template-columns: minmax(0, 1fr) auto;
        }
        input[type="range"] {
          grid-column: 1 / -1;
          width: 100%;
        }
      }
    `,
  ],
  template: `
    <section class="settings" aria-label="Host-user settings">
      <label>
        <span>Mouse sensitivity</span>
        <input
          #sensitivity
          type="range"
          min="0.25"
          max="2"
          step="0.05"
          [value]="settings().mouseSensitivity"
          (input)="sensitivityChanged.emit(sensitivity.valueAsNumber)"
        />
        <output>{{ settings().mouseSensitivity.toFixed(2) }}</output>
      </label>
      <label class="toggle">
        <span>Invert vertical look</span>
        <input
          #invertY
          type="checkbox"
          [checked]="settings().invertY"
          (change)="invertYChanged.emit(invertY.checked)"
        />
      </label>
      <label>
        <span>Effects volume</span>
        <input
          #volume
          type="range"
          min="0"
          max="1"
          step="0.05"
          [value]="settings().sfxVolume"
          (input)="sfxVolumeChanged.emit(volume.valueAsNumber)"
        />
        <output>{{ (settings().sfxVolume * 100).toFixed(0) }}%</output>
      </label>
      <label>
        <span>Flash intensity</span>
        <input
          #flashIntensity
          type="range"
          min="0"
          max="1"
          step="0.05"
          [value]="settings().flashIntensity"
          (input)="flashIntensityChanged.emit(flashIntensity.valueAsNumber)"
        />
        <output>{{ (settings().flashIntensity * 100).toFixed(0) }}%</output>
      </label>
      <label class="toggle">
        <span>Show game HUD</span>
        <input
          #hudVisible
          type="checkbox"
          [checked]="settings().hudVisible"
          (change)="hudVisibleChanged.emit(hudVisible.checked)"
        />
      </label>
      <label class="toggle">
        <span>Show renderer telemetry</span>
        <input
          #telemetryVisible
          type="checkbox"
          [checked]="settings().telemetryVisible"
          (change)="telemetryVisibleChanged.emit(telemetryVisible.checked)"
        />
      </label>

      @if (bindings().length > 0) {
        <h3>Current project bindings</h3>
        <dl>
          @for (binding of bindings(); track binding.action) {
            <dt>{{ binding.action }}</dt>
            <dd>{{ binding.binding }}</dd>
          }
        </dl>
        <p class="note">
          Binding display comes from the admitted Rust project. This demo does
          not advertise remapping because authored binding mutation is not yet a
          supported command.
        </p>
      } @else {
        <p class="note">
          Start or continue the game to inspect its admitted bindings. The
          main-menu settings screen does not invent a second binding source.
        </p>
      }
    </section>
  `,
})
export class SettingsPanelComponent {
  readonly settings = input.required<HostSettingsView>();
  readonly bindings = input.required<readonly KeyBindingView[]>();
  readonly sensitivityChanged = output<number>();
  readonly invertYChanged = output<boolean>();
  readonly sfxVolumeChanged = output<number>();
  readonly flashIntensityChanged = output<number>();
  readonly hudVisibleChanged = output<boolean>();
  readonly telemetryVisibleChanged = output<boolean>();
}

import {
  ChangeDetectionStrategy,
  Component,
  InjectionToken,
  computed,
  effect,
  inject,
  input,
  signal,
  untracked,
} from "@angular/core";
import type {
  StudioEntityInspectorContext,
  StudioEntityInspectorContribution,
  StudioEntityInspectorMutationPort,
  StudioEntityInspectorPanel,
} from "@rusty-engine/studio-editor-shell";

import {
  LOADING_BAY_WEAPON_AUTHORING_CONTRACT_ID,
  LOADING_BAY_WEAPON_AUTHORING_CONTRACT_VERSION,
  LOADING_BAY_WEAPON_COMPONENT_TYPE_ID,
  type LoadingBayWeaponAttackMode,
  type LoadingBayWeaponAuthoringClient,
  type LoadingBayWeaponCandidate,
  type LoadingBayWeaponReadout,
} from "./weapon-authoring-codec.js";
import {
  loadingBayWeaponInspectorContextKey,
  LoadingBayWeaponInspectorSession,
  type LoadingBayWeaponInspectorState,
} from "./weapon-inspector-session.js";

export const LOADING_BAY_WEAPON_AUTHORING_CLIENT =
  new InjectionToken<LoadingBayWeaponAuthoringClient>(
    "LoadingBayWeaponAuthoringClient",
  );

@Component({
  selector: "loading-bay-weapon-inspector-panel",
  standalone: true,
  template: `
    <section
      class="weapon-panel"
      aria-labelledby="loading-bay-weapon-title"
      data-visual-id="loading-bay-weapon-inspector"
    >
      <header>
        <div>
          <p class="eyebrow">Loading Bay component</p>
          <h3 id="loading-bay-weapon-title">Weapon</h3>
        </div>
        @if (status() !== null) {
          <span class="status" role="status">{{ status() }}</span>
        }
      </header>

      @if (loading()) {
        <p class="message" role="status">Reading canonical weapon…</p>
      } @else if (error() !== null && weapon() === null) {
        <div class="message error" role="alert">
          <strong>Weapon unavailable</strong>
          <span>{{ error() }}</span>
          <button type="button" (click)="retry()">Retry</button>
        </div>
      } @else if (weapon(); as currentWeapon) {
        @if (error() !== null) {
          <div class="message error" role="alert">
            <strong>Weapon edit rejected</strong>
            <span>{{ error() }}</span>
          </div>
        }
        <dl class="identity">
          <div>
            <dt>Item</dt>
            <dd>{{ currentWeapon.itemDefinitionId }}</dd>
          </div>
          <div>
            <dt>Owner</dt>
            <dd>{{ currentWeapon.ownerEntityId }}</dd>
          </div>
          <div>
            <dt>Inventory owner</dt>
            <dd>{{ currentWeapon.binding.inventoryOwnerEntityId }}</dd>
          </div>
          <div>
            <dt>Initial slot</dt>
            <dd>{{ currentWeapon.binding.slotIndex + 1 }}</dd>
          </div>
          <div>
            <dt>Starting quantity</dt>
            <dd>{{ currentWeapon.binding.startingQuantity }}</dd>
          </div>
          <div>
            <dt>Initially equipped</dt>
            <dd>
              {{ currentWeapon.binding.initiallyEquipped ? "Yes" : "No" }}
            </dd>
          </div>
        </dl>

        @if (draft(); as candidate) {
          <div class="form-grid">
            <label>
              Attack mode
              <select
                data-visual-id="weapon-attack-mode"
                [value]="candidate.attackMode.mode"
                [disabled]="disabled()"
                (change)="setAttackMode($event)"
              >
                <option value="hitscan">Hitscan</option>
                <option value="spread">Spread</option>
                <option value="automatic">Automatic</option>
              </select>
            </label>

            <label>
              Damage
              <input
                data-visual-id="weapon-damage"
                type="number"
                min="0"
                step="1"
                [value]="candidate.damage"
                [disabled]="disabled()"
                (input)="setCandidateNumber('damage', $event)"
              />
            </label>

            <label>
              Maximum distance
              <input
                type="number"
                min="0"
                step="0.1"
                [value]="candidate.maxDistance"
                [disabled]="disabled()"
                (input)="setCandidateNumber('maxDistance', $event)"
              />
            </label>

            <label>
              Cooldown ticks
              <input
                type="number"
                min="0"
                step="1"
                [value]="candidate.cooldownTicks"
                [disabled]="disabled()"
                (input)="setCandidateNumber('cooldownTicks', $event)"
              />
            </label>

            <label>
              Ammunition item
              <input
                type="text"
                [value]="candidate.ammunitionItemId"
                [disabled]="disabled()"
                (input)="setCandidateText('ammunitionItemId', $event)"
              />
            </label>

            <label>
              Ammunition cost
              <input
                type="number"
                min="0"
                step="1"
                [value]="candidate.ammunitionCost"
                [disabled]="disabled()"
                (input)="setCandidateNumber('ammunitionCost', $event)"
              />
            </label>

            @if (candidate.attackMode.mode === "spread") {
              <label>
                Pellet count
                <input
                  type="number"
                  min="0"
                  step="1"
                  [value]="candidate.attackMode.pelletCount"
                  [disabled]="disabled()"
                  (input)="setSpreadNumber('pelletCount', $event)"
                />
              </label>
              <label>
                Spread degrees
                <input
                  type="number"
                  min="0"
                  step="0.1"
                  [value]="candidate.attackMode.spreadDegrees"
                  [disabled]="disabled()"
                  (input)="setSpreadNumber('spreadDegrees', $event)"
                />
              </label>
            }

            <fieldset>
              <legend>Muzzle offset</legend>
              @for (axis of muzzleAxes; track axis.index) {
                <label>
                  {{ axis.label }}
                  <input
                    type="number"
                    step="0.01"
                    [value]="candidate.muzzleOffset[axis.index]"
                    [disabled]="disabled()"
                    (input)="setMuzzleOffset(axis.index, $event)"
                  />
                </label>
              }
            </fieldset>

            <label class="wide">
              Presentation identity
              <input
                type="text"
                [value]="candidate.presentation"
                [disabled]="disabled()"
                (input)="setCandidateText('presentation', $event)"
              />
            </label>
          </div>

          <footer>
            <button
              type="button"
              class="secondary"
              [disabled]="disabled() || !dirty()"
              (click)="resetDraft()"
            >
              Reset
            </button>
            <button
              type="button"
              data-visual-id="weapon-save"
              [disabled]="disabled() || !dirty() || !draftIsStructural()"
              (click)="save()"
            >
              {{ saving() ? "Saving…" : "Save weapon" }}
            </button>
          </footer>
        }
      }
    </section>
  `,
  styles: `
    :host {
      display: block;
    }

    .weapon-panel {
      display: grid;
      gap: 0.85rem;
      padding-block: 0.25rem 0.5rem;
    }

    header,
    footer {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 0.75rem;
    }

    h3,
    p {
      margin: 0;
    }

    h3 {
      font-size: 1rem;
    }

    .eyebrow,
    dt,
    .status {
      color: var(--rusty-studio-muted, #99a6b5);
      font-size: 0.72rem;
      letter-spacing: 0.04em;
      text-transform: uppercase;
    }

    .identity {
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 0.5rem;
      margin: 0;
    }

    .identity div {
      min-width: 0;
      padding: 0.55rem;
      border: 1px solid var(--rusty-studio-border, #34404e);
      border-radius: 0.3rem;
      background: var(--rusty-studio-chrome, #18202a);
    }

    dd {
      overflow-wrap: anywhere;
      margin: 0.2rem 0 0;
      font:
        0.78rem/1.3 ui-monospace,
        monospace;
    }

    .form-grid {
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 0.65rem;
    }

    label,
    fieldset {
      display: grid;
      min-width: 0;
      gap: 0.3rem;
      margin: 0;
      color: var(--rusty-studio-muted, #b6c0cb);
      font-size: 0.78rem;
    }

    fieldset,
    .wide {
      grid-column: 1 / -1;
    }

    fieldset {
      grid-template-columns: repeat(3, minmax(0, 1fr));
      padding: 0.55rem;
      border: 1px solid var(--rusty-studio-border, #34404e);
      border-radius: 0.3rem;
    }

    legend {
      padding-inline: 0.25rem;
    }

    input,
    select,
    button {
      box-sizing: border-box;
      min-width: 0;
      min-height: 2rem;
      border: 1px solid var(--rusty-studio-border, #435163);
      border-radius: 0.25rem;
      background: var(--rusty-studio-control, #111820);
      color: var(--rusty-studio-ink, #edf2f7);
      font: inherit;
    }

    input,
    select {
      width: 100%;
      padding: 0.35rem 0.45rem;
    }

    button {
      padding: 0.35rem 0.65rem;
      background: var(--rusty-studio-accent, #b56f34);
      border-color: var(--rusty-studio-accent, #b56f34);
      cursor: pointer;
    }

    button.secondary {
      background: transparent;
      border-color: var(--rusty-studio-border, #435163);
    }

    button:disabled,
    input:disabled,
    select:disabled {
      cursor: not-allowed;
      opacity: 0.55;
    }

    .message {
      display: grid;
      gap: 0.5rem;
      padding: 0.7rem;
      border: 1px solid var(--rusty-studio-border, #34404e);
      border-radius: 0.3rem;
    }

    .message.error {
      border-color: var(--rusty-studio-warning, #b94b4b);
      color: var(--rusty-studio-warning, #ffd3d3);
    }

    @media (max-width: 36rem) {
      .identity,
      .form-grid {
        grid-template-columns: 1fr;
      }

      fieldset,
      .wide {
        grid-column: auto;
      }
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class LoadingBayWeaponInspectorPanelComponent
  implements StudioEntityInspectorPanel
{
  readonly context = input.required<StudioEntityInspectorContext>();
  readonly mutationPort = input.required<StudioEntityInspectorMutationPort>();
  readonly weapon = signal<LoadingBayWeaponReadout | null>(null);
  readonly draft = signal<LoadingBayWeaponCandidate | null>(null);
  readonly loading = signal(true);
  readonly saving = signal(false);
  readonly error = signal<string | null>(null);
  readonly status = signal<string | null>(null);
  readonly #readNonce = signal(0);
  readonly #sessionKey = computed(() =>
    loadingBayWeaponInspectorContextKey(this.context()),
  );
  readonly muzzleAxes = [
    { index: 0, label: "X" },
    { index: 1, label: "Y" },
    { index: 2, label: "Z" },
  ] as const;
  readonly disabled = computed(() => this.context().busy || this.saving());
  readonly dirty = computed(() => {
    const current = this.weapon();
    const candidate = this.draft();
    return (
      current !== null &&
      candidate !== null &&
      JSON.stringify(current.definition) !== JSON.stringify(candidate)
    );
  });
  readonly draftIsStructural = computed(() => {
    const candidate = this.draft();
    if (candidate === null) return false;
    return (
      nonNegativeSafeInteger(candidate.damage) &&
      finiteNonNegative(candidate.maxDistance) &&
      nonNegativeSafeInteger(candidate.cooldownTicks) &&
      nonNegativeSafeInteger(candidate.ammunitionCost) &&
      candidate.ammunitionItemId.length > 0 &&
      candidate.presentation.length > 0 &&
      candidate.muzzleOffset.every(Number.isFinite) &&
      (candidate.attackMode.mode !== "spread" ||
        (nonNegativeSafeInteger(candidate.attackMode.pelletCount) &&
          finiteNonNegative(candidate.attackMode.spreadDegrees)))
    );
  });

  readonly #client = inject(LOADING_BAY_WEAPON_AUTHORING_CLIENT);
  #session: LoadingBayWeaponInspectorSession | null = null;
  #sessionWeapon: LoadingBayWeaponReadout | null = null;

  constructor() {
    effect((onCleanup) => {
      this.#sessionKey();
      this.#readNonce();
      const context = untracked(this.context);
      const session = new LoadingBayWeaponInspectorSession(
        this.#client,
        context,
        untracked(this.mutationPort),
        (state) => this.applySessionState(state),
      );
      this.#session = session;
      this.#sessionWeapon = null;
      session.load();
      onCleanup(() => {
        session.dispose();
        if (this.#session === session) this.#session = null;
      });
    });
  }

  retry(): void {
    this.#readNonce.update((nonce) => nonce + 1);
  }

  resetDraft(): void {
    const weapon = this.weapon();
    if (weapon !== null) this.draft.set(cloneCandidate(weapon.definition));
    this.status.set(null);
    this.error.set(null);
  }

  setAttackMode(event: Event): void {
    const mode = selectValue(event);
    const attackMode: LoadingBayWeaponAttackMode =
      mode === "spread"
        ? { mode, pelletCount: 7, spreadDegrees: 8 }
        : mode === "automatic"
          ? { mode }
          : { mode: "hitscan" };
    this.patchDraft({ attackMode });
  }

  setCandidateNumber(
    field: "damage" | "maxDistance" | "cooldownTicks" | "ammunitionCost",
    event: Event,
  ): void {
    this.patchDraft({ [field]: numberValue(event) });
  }

  setCandidateText(
    field: "ammunitionItemId" | "presentation",
    event: Event,
  ): void {
    this.patchDraft({ [field]: inputValue(event) });
  }

  setSpreadNumber(field: "pelletCount" | "spreadDegrees", event: Event): void {
    const candidate = this.draft();
    if (candidate?.attackMode.mode !== "spread") return;
    this.patchDraft({
      attackMode: {
        ...candidate.attackMode,
        [field]: numberValue(event),
      },
    });
  }

  setMuzzleOffset(index: 0 | 1 | 2, event: Event): void {
    const candidate = this.draft();
    if (candidate === null) return;
    const muzzleOffset: [number, number, number] = [...candidate.muzzleOffset];
    muzzleOffset[index] = numberValue(event);
    this.patchDraft({ muzzleOffset });
  }

  save(): void {
    const candidate = this.draft();
    const weapon = this.weapon();
    if (
      candidate === null ||
      weapon === null ||
      this.disabled() ||
      !this.dirty() ||
      !this.draftIsStructural()
    ) {
      return;
    }

    this.#session?.save(candidate);
  }

  private patchDraft(patch: Partial<LoadingBayWeaponCandidate>): void {
    const candidate = this.draft();
    if (candidate === null || this.disabled()) return;
    this.draft.set({ ...candidate, ...patch });
    this.status.set(null);
    this.error.set(null);
  }

  private applySessionState(state: LoadingBayWeaponInspectorState): void {
    this.loading.set(state.loading);
    this.saving.set(state.saving);
    this.error.set(state.error);
    this.status.set(state.status);
    this.weapon.set(state.weapon);
    if (state.weapon !== this.#sessionWeapon) {
      this.#sessionWeapon = state.weapon;
      this.draft.set(
        state.weapon === null ? null : cloneCandidate(state.weapon.definition),
      );
    }
  }
}

export const LOADING_BAY_WEAPON_INSPECTOR_CONTRIBUTION: StudioEntityInspectorContribution =
  Object.freeze({
    componentTypeId: LOADING_BAY_WEAPON_COMPONENT_TYPE_ID,
    contract: Object.freeze({
      contractId: LOADING_BAY_WEAPON_AUTHORING_CONTRACT_ID,
      contractVersion: LOADING_BAY_WEAPON_AUTHORING_CONTRACT_VERSION,
    }),
    title: "Loading Bay Weapon",
    order: 200,
    panel: LoadingBayWeaponInspectorPanelComponent,
    dataVisualId: "loading-bay-weapon-component",
  });

function cloneCandidate(
  candidate: LoadingBayWeaponCandidate,
): LoadingBayWeaponCandidate {
  return {
    ...candidate,
    attackMode: { ...candidate.attackMode },
    muzzleOffset: [...candidate.muzzleOffset],
  };
}

function finiteNonNegative(value: number): boolean {
  return Number.isFinite(value) && value >= 0;
}

function nonNegativeSafeInteger(value: number): boolean {
  return Number.isSafeInteger(value) && value >= 0;
}

function inputValue(event: Event): string {
  return (event.target as HTMLInputElement).value;
}

function selectValue(event: Event): string {
  return (event.target as HTMLSelectElement).value;
}

function numberValue(event: Event): number {
  const value = inputValue(event).trim();
  return value.length === 0 ? Number.NaN : Number(value);
}

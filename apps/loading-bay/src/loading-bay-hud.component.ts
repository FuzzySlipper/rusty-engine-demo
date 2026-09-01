import {
  ChangeDetectionStrategy,
  Component,
  OnDestroy,
  inject,
  signal,
} from "@angular/core";
import type { RustyApplicationUiProjectionEnvelope } from "@rusty-engine/application-host";
import { ENGINE_APPLICATION } from "./engine-application";

interface LoadingBayHudSnapshot {
  readonly health: number;
  readonly armor: number;
  readonly bullets: number;
  readonly shells: number;
  readonly generation: number;
  readonly step: number;
  readonly complete: boolean;
  readonly facts: readonly LoadingBayHudFact[];
  readonly droppedFacts: number;
  readonly pendingSchedules: number;
  readonly exitVisibility: boolean;
  readonly exitVisibilityRevision: number;
  readonly presentationBillboards: number;
  readonly animationCue: string;
  readonly effectsMuted: boolean;
  readonly effectsVolume: number;
  readonly updateMode: string;
  readonly lifecycle: string;
  readonly admittedSteps: number;
  readonly droppedSteps: number;
}

interface LoadingBayHudFact {
  readonly kind: string;
}

const EMPTY_READOUT: LoadingBayHudSnapshot = {
  health: 0,
  armor: 0,
  bullets: 0,
  shells: 0,
  generation: 0,
  step: 0,
  complete: false,
  facts: [],
  droppedFacts: 0,
  pendingSchedules: 0,
  exitVisibility: false,
  exitVisibilityRevision: 0,
  presentationBillboards: 0,
  animationCue: "",
  effectsMuted: false,
  effectsVolume: 0,
  updateMode: "",
  lifecycle: "",
  admittedSteps: 0,
  droppedSteps: 0,
};

/** A disposable DOM readout over the immutable C# Engine UI projection. */
@Component({
  changeDetection: ChangeDetectionStrategy.OnPush,
  selector: "red-loading-bay-hud",
  standalone: true,
  styles: [
    `
      :host {
        display: block;
        inset: 0;
        pointer-events: none;
        position: fixed;
      }
      .hud {
        color: #e8f3ef;
        font:
          600 12px/1.35 ui-monospace,
          SFMono-Regular,
          Consolas,
          monospace;
        inset: 0;
        position: absolute;
        text-shadow: 0 2px 4px #000;
      }
      .readout {
        background: #071012cc;
        border: 1px solid #6f948a;
        display: grid;
        gap: 4px;
        letter-spacing: 0.06em;
        padding: 10px 12px;
        position: absolute;
        text-transform: uppercase;
      }
      .identity {
        left: 20px;
        top: 18px;
      }
      .vitals {
        align-items: baseline;
        bottom: 28px;
        display: flex;
        gap: 14px;
        right: 20px;
      }
      .vitals strong {
        color: #a8f7d5;
        font-size: 24px;
      }
      .status {
        bottom: 28px;
        color: #aec7c0;
        left: 20px;
        max-width: min(42rem, calc(100vw - 40px));
      }
      .complete {
        color: #a8f7d5;
      }
      .fault {
        color: #ffb4a9;
      }
    `,
  ],
  template: `
    <section class="hud" aria-label="Loading Bay game readout">
      <div class="readout identity">
        <span>DOOM E1M1 / HANGAR</span>
        <strong [class.complete]="snapshot().complete">
          {{ snapshot().complete ? "EXIT SECURED" : "ENGINE RUNTIME ACTIVE" }}
        </strong>
        <span
          >{{ snapshot().updateMode }} · {{ snapshot().lifecycle }} · GEN
          {{ snapshot().generation }} · STEP {{ snapshot().step }}</span
        >
      </div>

      <div class="readout vitals" aria-label="Current player vitals">
        <span
          >HEALTH <strong>{{ snapshot().health }}</strong></span
        >
        <span
          >ARMOR <strong>{{ snapshot().armor }}</strong></span
        >
        <span>BULLETS <strong>{{ snapshot().bullets }}</strong></span>
        <span>SHELLS <strong>{{ snapshot().shells }}</strong></span>
      </div>

      <div class="readout status" aria-live="polite">
        @if (projectionFault() !== null) {
          <span class="fault"
            >HUD projection unavailable: {{ projectionFault() }}</span
          >
        } @else if (ready()) {
          <span>
            FACTS {{ snapshot().facts.length }} · DROPPED
            {{ snapshot().droppedFacts }} · SCHEDULES
            {{ snapshot().pendingSchedules }} · EXIT
            {{ snapshot().exitVisibility ? "VISIBLE" : "OCCLUDED" }} ({{
              snapshot().exitVisibilityRevision
            }}) · BILLBOARDS {{ snapshot().presentationBillboards }} · AUDIO
            {{ snapshot().effectsMuted ? "MUTED" : snapshot().effectsVolume }}
            · ADMITTED {{ snapshot().admittedSteps }} · DROPPED STEPS
            {{ snapshot().droppedSteps }}
            @if (snapshot().animationCue) {
              · CUE {{ snapshot().animationCue }}
            }
          </span>
        } @else {
          <span>WAITING FOR ENGINE HUD PROJECTION…</span>
        }
      </div>
    </section>
  `,
})
export class LoadingBayHudComponent implements OnDestroy {
  protected readonly snapshot = signal<LoadingBayHudSnapshot>(EMPTY_READOUT);
  protected readonly ready = signal(false);
  protected readonly projectionFault = signal<string | null>(null);

  private readonly application = inject(ENGINE_APPLICATION);
  private readonly unsubscribe = this.application.projection?.subscribe(
    (envelope) => {
      if (envelope === null) {
        this.ready.set(false);
        return;
      }
      const value = readHudSnapshot(envelope);
      if (value === null) {
        this.projectionFault.set("rejected malformed product value");
        return;
      }
      this.snapshot.set(value);
      this.projectionFault.set(null);
      this.ready.set(true);
    },
  );

  ngOnDestroy(): void {
    this.unsubscribe?.();
  }
}

function readHudSnapshot(
  envelope: RustyApplicationUiProjectionEnvelope,
): LoadingBayHudSnapshot | null {
  if (
    envelope.stream !== "loading-bay.hud" ||
    envelope.contract !== "loading-bay.hud.snapshot.v1"
  ) {
    return null;
  }
  const value = envelope.value;
  if (!isRecord(value)) return null;
  const health = finite(value.health);
  const armor = finite(value.armor);
  const bullets = finite(value.bullets);
  const shells = finite(value.shells);
  const generation = finite(value.generation);
  const step = finite(value.step);
  const droppedFacts = finite(value.droppedFacts);
  const pendingSchedules = finite(value.pendingSchedules);
  const exitVisibilityRevision = finite(value.exitVisibilityRevision);
  const presentationBillboards = finite(value.presentationBillboards);
  const effectsVolume = finite(value.effectsVolume);
  const admittedSteps = finite(value.admittedSteps);
  const droppedSteps = finite(value.droppedSteps);
  if (
    health === null ||
    armor === null ||
    bullets === null ||
    shells === null ||
    generation === null ||
    step === null ||
    droppedFacts === null ||
    pendingSchedules === null ||
    exitVisibilityRevision === null ||
    presentationBillboards === null ||
    effectsVolume === null ||
    admittedSteps === null ||
    droppedSteps === null ||
    typeof value.complete !== "boolean" ||
    typeof value.exitVisibility !== "boolean" ||
    typeof value.animationCue !== "string" ||
    typeof value.updateMode !== "string" ||
    typeof value.lifecycle !== "string" ||
    typeof value.effectsMuted !== "boolean" ||
    !Array.isArray(value.facts) ||
    !value.facts.every(isHudFact)
  ) {
    return null;
  }
  return {
    health,
    armor,
    bullets,
    shells,
    generation,
    step,
    complete: value.complete,
    facts: value.facts,
    droppedFacts,
    pendingSchedules,
    exitVisibility: value.exitVisibility,
    exitVisibilityRevision,
    presentationBillboards,
    animationCue: value.animationCue,
    effectsMuted: value.effectsMuted,
    effectsVolume,
    updateMode: value.updateMode,
    lifecycle: value.lifecycle,
    admittedSteps,
    droppedSteps,
  };
}

function isRecord(value: unknown): value is Readonly<Record<string, unknown>> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function finite(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function isHudFact(value: unknown): value is LoadingBayHudFact {
  return isRecord(value) && typeof value.kind === "string";
}

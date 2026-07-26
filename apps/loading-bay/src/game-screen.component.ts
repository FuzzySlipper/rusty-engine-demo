import {
  ChangeDetectionStrategy,
  Component,
  type AfterViewInit,
  type OnDestroy,
  computed,
  inject,
  signal,
} from "@angular/core";
import { Router, RouterLink } from "@angular/router";
import {
  mountLoadingBayGame,
  type LoadingBayGameHandle,
  type LoadingBayPresentationSnapshot,
} from "@rusty-engine-demo/game-runtime";
import { browserDocumentEffects } from "@rusty-engine-demo/platform";
import {
  CombatLogComponent,
  type CombatLogEntryView,
} from "@rusty-engine-demo/ui-combat-log";
import { CompassComponent } from "@rusty-engine-demo/ui-compass";

const INITIAL_SNAPSHOT: LoadingBayPresentationSnapshot = {
  ammoCapacity: 0,
  ammoRemaining: 0,
  encounterState: "loading",
  events: [],
  headingDegrees: 0,
};

@Component({
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [CombatLogComponent, CompassComponent, RouterLink],
  selector: "red-game-screen",
  standalone: true,
  template: `
    <main class="game-shell">
      <section
        class="viewport-card"
        aria-label="Three-dimensional encounter view"
      >
        <canvas
          id="viewport"
          width="1600"
          height="900"
          tabindex="0"
          aria-label="Loading Bay first-person viewport. Click to capture the pointer."
        ></canvas>
        <div
          id="feedback-layer"
          class="feedback-layer"
          aria-live="polite"
        ></div>
        <div class="viewport-vignette" aria-hidden="true"></div>
        <div class="reticle" aria-hidden="true"></div>

        <header class="hud-top">
          <div class="mission">
            <p>Loading Bay 03</p>
            <strong id="encounter-state">LOADING</strong>
            <span id="revision">REV —</span>
          </div>
          <red-compass [headingDegrees]="snapshot().headingDegrees" />
          <a class="diagnostics-link" routerLink="/diagnostics">Diagnostics</a>
        </header>

        <div class="hud-left">
          <red-combat-log [entries]="combatEntries()" />
        </div>

        <div class="hud-right">
          <div class="scene-caption">
            <span>EXIT / NORTH BULKHEAD</span>
            <strong id="door-caption">LOCKED</strong>
          </div>
          <div class="ammo" aria-label="Primary weapon ammunition">
            <strong>{{ snapshot().ammoRemaining }}</strong>
            <span>/ {{ snapshot().ammoCapacity }}</span>
          </div>
        </div>

        <p class="pointer-help">
          CLICK TO CAPTURE · WASD MOVE · MOUSE LOOK · PRIMARY FIRE
        </p>
        <div
          id="feedback-audio-status"
          class="feedback-audio-status"
          data-state="inactive"
        >
          AUDIO WAITING
        </div>
      </section>

      <details class="diagnostic-drawer">
        <summary>Runtime diagnostics and authored-content actions</summary>
        <div class="operations">
          <article class="status-panel">
            <p class="section-label">Rust-owned state</p>
            <div id="enemy-list" class="enemy-list"></div>
            <div class="motion-readout">
              <span>Spatial probe</span
              ><strong id="motion-state">MOVING</strong>
            </div>
            <div class="motion-readout">
              <span>Enemy navigation</span
              ><strong id="navigation-state">FOLLOWING</strong>
            </div>
            <div class="motion-readout">
              <span>Player controller</span
              ><strong id="player-motion-state">IDLE</strong>
            </div>
            <div class="motion-readout">
              <span>Combat resolution</span
              ><strong id="combat-state">READY</strong>
            </div>
            <div class="motion-readout">
              <span>Extraction beacon</span
              ><strong id="beacon-state">STANDBY</strong>
            </div>
            <div class="pose-readout" id="player-pose">
              Awaiting Rust projection
            </div>
            <div class="pose-readout" id="weapon-state">
              Awaiting weapon projection
            </div>
            <div class="pose-readout" id="environment-state">
              Awaiting environment projection
            </div>
            <div class="pose-readout" id="voxel-state">
              Awaiting voxel projection
            </div>
            <div id="renderer-telemetry" class="renderer-telemetry"></div>
          </article>

          <article class="control-panel">
            <p class="section-label">Player and authoring actions</p>
            <div class="button-row">
              <button type="button" id="primary-fire">Fire Primary</button>
              <button type="button" id="activate-beacon">
                Activate Extraction
              </button>
              <button type="button" id="remove-voxel">Remove Voxel</button>
              <button type="button" id="place-voxel">Place Voxel</button>
              <button type="button" id="reset" class="quiet">Reset</button>
            </div>
            <label class="persist-control">
              <input type="checkbox" id="persist-voxel-edit" />
              Save voxel edit to authored project
            </label>
          </article>

          <article class="event-panel">
            <p class="section-label">Raw committed facts</p>
            <ol id="event-list" class="event-list">
              <li>Awaiting action</li>
            </ol>
          </article>
        </div>
      </details>

      <footer>
        <span id="renderer-status">Renderer starting…</span>
        <span id="smoke-result" data-status="idle">Product proof idle</span>
      </footer>
    </main>
  `,
})
export class GameScreenComponent implements AfterViewInit, OnDestroy {
  protected readonly snapshot = signal(INITIAL_SNAPSHOT);
  protected readonly combatEntries = computed<readonly CombatLogEntryView[]>(
    () =>
      this.snapshot()
        .events.slice(-7)
        .map((event, index) => ({
          id: index,
          severity: severityFor(event),
          source:
            event.includes("Combat") || event.includes("Damage")
              ? "COMBAT"
              : "SYSTEM",
          text: event,
        })),
  );

  private readonly documentEffects = browserDocumentEffects();
  private readonly router = inject(Router);
  private destroyed = false;
  private handle: LoadingBayGameHandle | null = null;

  ngAfterViewInit(): void {
    this.documentEffects.setTitle("Rusty Engine — Loading Bay");
    this.documentEffects.setRootClass("game-route-active", true);
    document.body.dataset.rendererLifecycle = "mounting";
    void mountLoadingBayGame({
      onProjection: (snapshot) => {
        this.snapshot.set(snapshot);
      },
    })
      .then(async (handle) => {
        if (this.destroyed) {
          await handle.dispose();
          return;
        }
        this.handle = handle;
        document.body.dataset.rendererLifecycle = "mounted";
        if (new URLSearchParams(location.search).has("lifecycle-smoke")) {
          await this.router.navigateByUrl("/diagnostics");
        }
      })
      .catch((error: unknown) => {
        document.body.dataset.rendererLifecycle = "failed";
        document.body.dataset.runtimeError =
          error instanceof Error ? error.message : String(error);
      });
  }

  ngOnDestroy(): void {
    this.destroyed = true;
    this.documentEffects.setRootClass("game-route-active", false);
    const handle = this.handle;
    this.handle = null;
    if (handle !== null) {
      document.body.dataset.rendererLifecycle = "disposed";
      void handle.dispose();
    }
  }
}

function severityFor(event: string): CombatLogEntryView["severity"] {
  if (event.includes("Rejected") || event.includes("Blocked")) {
    return "miss";
  }
  if (
    event.includes("Combat") ||
    event.includes("Damage") ||
    event.includes("Defeated")
  ) {
    return "hit";
  }
  return "system";
}

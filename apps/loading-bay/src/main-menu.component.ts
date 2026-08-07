import {
  ChangeDetectionStrategy,
  Component,
  inject,
  signal,
} from "@angular/core";
import { Router, RouterLink } from "@angular/router";
import type {
  LoadingBaySaveSlot,
  LoadingBaySaveSlotId,
} from "@rusty-engine-demo/game-runtime";
import {
  browserDocumentEffects,
  browserHostUserSettingsRepository,
} from "@rusty-engine-demo/platform";

@Component({
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink],
  selector: "red-main-menu",
  standalone: true,
  template: `
    <main class="main-menu-screen">
      <section class="main-menu-card" aria-labelledby="game-title">
        <header>
          <p class="eyebrow">Rusty Engine downstream reference game</p>
          <h1 id="game-title">LOADING BAY</h1>
          <p>
            Clear the security encounter, recover field equipment, and bring the
            extraction beacon online.
          </p>
        </header>

        <nav aria-label="Main menu">
          <button type="button" class="primary" (click)="newGame()">
            New game
          </button>
          <button
            type="button"
            [disabled]="!continueAvailable()"
            [attr.title]="
              continueAvailable()
                ? 'Continue the live Rust-owned session'
                : 'No game has been started in this browser'
            "
            (click)="continueGame()"
          >
            Continue
          </button>
          <a routerLink="/settings">Settings</a>
        </nav>

        <section aria-label="Authored projects">
          <h2>Scenes</h2>
          <ul class="scene-list">
            <li>
              <button type="button" class="quiet" (click)="startScene('loading-bay')">
                Loading Bay — encounter &amp; beacon
              </button>
              <small>Default Rust-owned project at content/projects/loading-bay.project.json</small>
            </li>
            <li>
              <button type="button" class="quiet" (click)="startScene('relay-annex')">
                Relay Annex — data-only variation
              </button>
              <small>Same services, different layout at relay-annex.project.json</small>
            </li>
            <li>
              <button type="button" class="quiet" (click)="startScene('doom-e1m1')">
                Doom E1M1 — Hangar (voxel showcase)
              </button>
              <small>Textured-voxel showcase at content/projects/doom-e1m1.project.json — 54 VTX6 materials, single VoxelAsset</small>
            </li>
          </ul>
          <p class="project-hint">
            The host selects the authored project at startup via
            <code>cargo run -p loading-bay-game --bin browser-host -- --project content/projects/&lt;name&gt;.project.json</code>.
            The card navigates to <code>/game?project=&lt;name&gt;</code> so the single shared
            <code>RendererSurface</code> and projection adapter remain the only scene owners.
          </p>
        </section>

        <p
          class="availability"
          [attr.data-session-readiness]="continueReadiness()"
        >
          {{ continueMessage() }}
        </p>
      </section>
      <footer>
        <span>WASD + mouse</span>
        <span>Three authored weapons</span>
        <span>Deterministic fixed simulation</span>
      </footer>
    </main>
  `,
})
export class MainMenuComponent {
  protected readonly continueAvailable = signal(false);
  protected readonly continueMessage = signal(
    "Checking the live Rust-owned session…",
  );
  protected readonly continueReadiness = signal<ContinueReadiness>("checking");

  private readonly documentEffects = browserDocumentEffects();
  private readonly repository = browserHostUserSettingsRepository();
  private readonly router = inject(Router);
  private readonly continueTarget = signal<ContinueTarget | null>(null);

  constructor() {
    this.documentEffects.setTitle("Loading Bay — Main menu");
    void this.resolveContinueAvailability();
  }

  protected newGame(): void {
    void this.router.navigate(["/game"], {
      queryParams: { mode: "new" },
    });
  }

  protected startScene(scene: "loading-bay" | "relay-annex" | "doom-e1m1"): void {
    void this.router.navigate(["/game"], {
      queryParams: { mode: "new", project: scene },
    });
  }

  protected continueGame(): void {
    const target = this.continueTarget();
    if (target === null) {
      return;
    }
    void this.router.navigate(
      ["/game"],
      target.kind === "live"
        ? { queryParams: { mode: "continue" } }
        : {
            queryParams: {
              mode: "load",
              revision: target.storageRevision,
              slot: target.slot,
            },
          },
    );
  }

  private async resolveContinueAvailability(): Promise<void> {
    try {
      const response = await fetch("/api/menu-state", { cache: "no-store" });
      if (!response.ok) {
        throw new Error(`host state returned ${String(response.status)}`);
      }
      const value: unknown = await response.json();
      const hostSessionId =
        typeof value === "object" &&
        value !== null &&
        "hostSessionId" in value &&
        typeof value.hostSessionId === "string"
          ? value.hostSessionId
          : "";
      const live = this.repository.hasContinueSession(hostSessionId);
      if (live) {
        this.continueTarget.set({ kind: "live" });
        this.continueAvailable.set(true);
        this.continueReadiness.set("verified-live");
        this.continueMessage.set(
          "Continue reconnects to the verified live Rust-owned session.",
        );
        return;
      }
      const save = newestAvailableSave(value);
      if (save !== null && save.storageRevision !== null) {
        this.continueTarget.set({
          kind: "save",
          slot: save.slot,
          storageRevision: save.storageRevision,
        });
        this.continueAvailable.set(true);
        this.continueReadiness.set("verified-save");
        this.continueMessage.set(
          `Continue restores ${save.metadata?.displayName ?? saveSlotLabel(save.slot)} at tick ${String(save.metadata?.tick ?? 0)} from Rust-owned storage.`,
        );
        return;
      }
      this.continueTarget.set(null);
      this.continueAvailable.set(false);
      this.continueReadiness.set("verified-none");
      this.continueMessage.set(
        "No verified live session or compatible save exists. Start a new game.",
      );
    } catch {
      this.continueTarget.set(null);
      this.continueAvailable.set(false);
      this.continueReadiness.set("unavailable");
      this.continueMessage.set(
        "Continue is unavailable while the Rust host session cannot be verified.",
      );
    }
  }
}

type ContinueReadiness =
  | "checking"
  | "verified-live"
  | "verified-save"
  | "verified-none"
  | "unavailable";

type ContinueTarget =
  | { readonly kind: "live" }
  | {
      readonly kind: "save";
      readonly slot: LoadingBaySaveSlotId;
      readonly storageRevision: string;
    };

function newestAvailableSave(value: unknown): LoadingBaySaveSlot | null {
  if (
    typeof value !== "object" ||
    value === null ||
    !("saveSlots" in value) ||
    !Array.isArray(value.saveSlots)
  ) {
    return null;
  }
  const available = value.saveSlots
    .filter(isAvailableSave)
    .sort(
      (left, right) =>
        (right.metadata?.savedAtUnixMilliseconds ?? 0) -
        (left.metadata?.savedAtUnixMilliseconds ?? 0),
    );
  return available[0] ?? null;
}

function isAvailableSave(value: unknown): value is LoadingBaySaveSlot {
  return (
    typeof value === "object" &&
    value !== null &&
    "slot" in value &&
    (value.slot === "checkpoint" ||
      value.slot === "slot1" ||
      value.slot === "slot2" ||
      value.slot === "slot3") &&
    "compatibility" in value &&
    value.compatibility === "available" &&
    "storageRevision" in value &&
    typeof value.storageRevision === "string" &&
    "metadata" in value
  );
}

function saveSlotLabel(slot: LoadingBaySaveSlotId): string {
  switch (slot) {
    case "checkpoint":
      return "Checkpoint";
    case "slot1":
      return "Manual save 1";
    case "slot2":
      return "Manual save 2";
    case "slot3":
      return "Manual save 3";
  }
}

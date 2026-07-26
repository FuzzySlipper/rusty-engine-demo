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

        <p class="availability">
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
      const response = await fetch("/api/state", { cache: "no-store" });
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
        this.continueMessage.set(
          `Continue restores ${save.metadata?.displayName ?? saveSlotLabel(save.slot)} at tick ${String(save.metadata?.tick ?? 0)} from Rust-owned storage.`,
        );
        return;
      }
      this.continueTarget.set(null);
      this.continueAvailable.set(false);
      this.continueMessage.set(
        "No verified live session or compatible save exists. Start a new game.",
      );
    } catch {
      this.continueTarget.set(null);
      this.continueAvailable.set(false);
      this.continueMessage.set(
        "Continue is unavailable while the Rust host session cannot be verified.",
      );
    }
  }
}

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

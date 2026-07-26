import {
  ChangeDetectionStrategy,
  Component,
  inject,
  signal,
} from "@angular/core";
import { Router, RouterLink } from "@angular/router";
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

        @if (!continueAvailable()) {
          <p class="availability">
            {{ continueMessage() }}
          </p>
        } @else {
          <p class="availability">
            Continue reconnects to the current Rust-owned host session.
          </p>
        }
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
    if (!this.continueAvailable()) {
      return;
    }
    void this.router.navigate(["/game"], {
      queryParams: { mode: "continue" },
    });
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
      const available = this.repository.hasContinueSession(hostSessionId);
      this.continueAvailable.set(available);
      this.continueMessage.set(
        available
          ? "Continue reconnects to the current Rust-owned host session."
          : "No resumable game exists for this Rust host session. Start a new game.",
      );
    } catch {
      this.continueAvailable.set(false);
      this.continueMessage.set(
        "Continue is unavailable while the Rust host session cannot be verified.",
      );
    }
  }
}

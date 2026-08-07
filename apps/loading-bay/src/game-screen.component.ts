import {
  ChangeDetectionStrategy,
  Component,
  type AfterViewInit,
  type OnDestroy,
  computed,
  HostListener,
  inject,
  signal,
} from "@angular/core";
import { ActivatedRoute, Router, RouterLink } from "@angular/router";
import {
  mountLoadingBayGame,
  type LoadingBayGameHandle,
  type LoadingBayPresentationSnapshot,
  type LoadingBaySaveSlot,
  type LoadingBaySaveSlotId,
} from "@rusty-engine-demo/game-runtime";
import {
  browserDocumentEffects,
  browserHostUserSettingsRepository,
  type HostUserSettings,
} from "@rusty-engine-demo/platform";
import {
  CombatLogComponent,
  type CombatLogEntryView,
} from "@rusty-engine-demo/ui-combat-log";
import { CompassComponent } from "@rusty-engine-demo/ui-compass";
import {
  GameHotbarComponent,
  InventoryPanelComponent,
  SettingsPanelComponent,
  type GameHotbarSlotView,
  type HostSettingsView,
  type InventoryStackView,
  type InventoryWeaponView,
  type KeyBindingView,
} from "@rusty-engine-demo/ui-game-panels";

const INITIAL_SNAPSHOT: LoadingBayPresentationSnapshot = {
  ammoCapacity: 0,
  ammoRemaining: 0,
  armor: 0,
  bindings: {
    moveForward: "KeyW",
    moveBackward: "KeyS",
    moveLeft: "KeyA",
    moveRight: "KeyD",
    mouseLook: "pointer",
    primaryFire: "Mouse0",
    selectWeapon: ["Digit1", "Digit2", "Digit3"],
  },
  connected: false,
  doorState: "closed",
  equippedWeapon: null,
  encounterState: "loading",
  events: [],
  health: 0,
  headingDegrees: 0,
  hostSessionId: "",
  interactionPrompt: null,
  interactionTarget: null,
  inventoryCapacity: 0,
  inventoryStacks: [],
  lastRejection: null,
  levelComplete: false,
  levelCompletionPresentation: null,
  maxArmor: 0,
  maxHealth: 0,
  paused: false,
  restartAvailable: false,
  saveSlots: [],
  vitalityState: "alive",
  weaponItem: "",
  weaponPresentation: "",
  weaponSlots: [],
};

type GamePanel = "game" | "inventory" | "load" | "pause" | "save" | "settings";
type ConnectionState =
  | "connecting"
  | "connected"
  | "reconnecting"
  | "unavailable";

declare global {
  interface Window {
    __loadingBayAnimationCapture?: LoadingBayGameHandle["captureAnimation"];
  }
}

@Component({
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    CombatLogComponent,
    CompassComponent,
    GameHotbarComponent,
    InventoryPanelComponent,
    RouterLink,
    SettingsPanelComponent,
  ],
  selector: "red-game-screen",
  standalone: true,
  template: `
    <main class="game-shell">
      <section
        class="viewport-card"
        [class.hud-hidden]="!settings().hudVisible"
        aria-label="Three-dimensional encounter view"
      >
        <canvas
          id="viewport"
          width="1600"
          height="900"
          tabindex="0"
          [attr.inert]="modalActive() ? '' : null"
          aria-label="Loading Bay first-person viewport. Click to capture the pointer."
        ></canvas>
        <div
          id="feedback-layer"
          class="feedback-layer"
          aria-live="polite"
        ></div>
        <div class="viewport-vignette" aria-hidden="true"></div>
        <div class="reticle" aria-hidden="true"></div>

        <header class="hud-top" [attr.inert]="modalActive() ? '' : null">
          <div class="mission">
            <p>Loading Bay 03</p>
            <strong id="encounter-state">LOADING</strong>
            <span id="revision">REV —</span>
          </div>
          <red-compass [headingDegrees]="snapshot().headingDegrees" />
          <nav class="hud-actions" aria-label="Game screens">
            <button type="button" (click)="openInventory()">Inventory</button>
            <button type="button" (click)="openPause()">Pause</button>
            <a class="diagnostics-link" routerLink="/diagnostics"
              >Diagnostics</a
            >
          </nav>
        </header>

        <div class="hud-left" [attr.inert]="modalActive() ? '' : null">
          <red-combat-log [entries]="combatEntries()" />
        </div>

        <div class="hud-right" [attr.inert]="modalActive() ? '' : null">
          <div
            class="vitality"
            [attr.data-state]="snapshot().vitalityState"
            aria-label="Player health and armor"
          >
            <span>HEALTH</span>
            <strong
              >{{ snapshot().health }} / {{ snapshot().maxHealth }}</strong
            >
            <small
              >ARMOR {{ snapshot().armor }} / {{ snapshot().maxArmor }}</small
            >
          </div>
          <div class="scene-caption">
            <span>EXIT / NORTH BULKHEAD</span>
            <strong id="door-caption">LOCKED</strong>
          </div>
          <div class="ammo" aria-label="Primary weapon ammunition">
            <strong>{{ snapshot().ammoRemaining }}</strong>
            <span>/ {{ snapshot().ammoCapacity }}</span>
          </div>
          <div class="key-ring" aria-label="Carried access keys">
            @if (carriedKeys().length === 0) {
              <span>NO ACCESS KEYS</span>
            } @else {
              @for (key of carriedKeys(); track key) {
                <span>{{ itemLabel(key) }}</span>
              }
            }
          </div>
        </div>

        <div class="hud-hotbar" [attr.inert]="modalActive() ? '' : null">
          <red-game-hotbar
            [slots]="hotbarSlots()"
            [disabled]="
              actionBusy() ||
              snapshot().paused ||
              snapshot().vitalityState === 'dead'
            "
            (weaponSelected)="selectWeapon($event)"
          />
        </div>

        @if (snapshot().interactionPrompt !== null && panel() === "game") {
          <button
            type="button"
            class="interaction-prompt"
            [attr.inert]="modalActive() ? '' : null"
            [disabled]="actionBusy()"
            (click)="activateInteraction()"
          >
            {{ snapshot().interactionPrompt }}
          </button>
        }

        <p class="pointer-help">
          CLICK TO CAPTURE · WASD MOVE · MOUSE LOOK · PRIMARY FIRE · I INVENTORY
          · E INTERACT · ESC PAUSE
        </p>
        <div
          id="feedback-audio-status"
          class="feedback-audio-status"
          data-state="inactive"
        >
          AUDIO WAITING
        </div>

        @if (connectionState() !== "connected") {
          <section
            class="game-state-overlay"
            role="dialog"
            aria-modal="true"
            tabindex="-1"
            data-active-modal
            [attr.aria-busy]="
              connectionState() === 'connecting' ||
              connectionState() === 'reconnecting'
            "
            aria-live="polite"
          >
            <p class="section-label">Game session</p>
            <h1>{{ connectionState().toUpperCase() }}</h1>
            <p>{{ connectionMessage() }}</p>
            @if (connectionState() === "unavailable") {
              <button type="button" (click)="retryConnection()">Retry</button>
              <button type="button" class="quiet" (click)="returnToMenu()">
                Main menu
              </button>
            }
          </section>
        } @else if (snapshot().vitalityState === "dead" && panel() === "game") {
          <section
            class="game-state-overlay"
            role="dialog"
            aria-modal="true"
            aria-live="assertive"
            tabindex="-1"
            data-active-modal
          >
            <p class="section-label">Rust-owned vitality</p>
            <h1>PLAYER DOWN</h1>
            <p>
              Health reached zero. Movement, combat, and inventory mutations
              remain unavailable until an authoritative restart.
            </p>
            <button
              type="button"
              [disabled]="actionBusy() || checkpointSlot() === null"
              (click)="restoreCheckpoint()"
            >
              Restore checkpoint
            </button>
            <button
              type="button"
              [disabled]="actionBusy() || !snapshot().restartAvailable"
              (click)="restartGame()"
            >
              Restart loading bay
            </button>
            <button type="button" class="quiet" (click)="returnToMenu()">
              Main menu
            </button>
            @if (slotStatus() !== null) {
              <p class="action-rejection" role="alert">
                {{ slotStatus() }}
              </p>
            }
          </section>
        } @else if (snapshot().levelComplete && panel() === "game") {
          <section
            class="game-state-overlay"
            role="dialog"
            aria-modal="true"
            aria-live="assertive"
            tabindex="-1"
            data-active-modal
          >
            <p class="section-label">Rust-owned level result</p>
            <h1>LOADING BAY COMPLETE</h1>
            <p>
              {{
                snapshot().levelCompletionPresentation ??
                  "The authored exit has been secured."
              }}
            </p>
            <button
              type="button"
              [disabled]="actionBusy() || !snapshot().restartAvailable"
              (click)="restartGame()"
            >
              Restart loading bay
            </button>
            <button
              type="button"
              [disabled]="actionBusy() || !loadAvailable()"
              (click)="openLoadPanel()"
            >
              Load save
            </button>
            <button type="button" (click)="openSavePanel()">
              Save completed run
            </button>
            <button type="button" class="quiet" (click)="returnToMenu()">
              Main menu
            </button>
          </section>
        }

        @if (
          panel() === "game" &&
          snapshot().lastRejection !== null &&
          snapshot().vitalityState !== "dead"
        ) {
          <p class="action-rejection game-action-rejection" role="alert">
            {{ snapshot().lastRejection }}
          </p>
        }

        @if (panel() !== "game") {
          <section
            class="game-panel-overlay"
            role="dialog"
            aria-modal="true"
            tabindex="-1"
            data-active-modal
            [attr.aria-label]="panelTitle()"
          >
            <article class="game-panel">
              <header>
                <div>
                  <p class="section-label">Loading Bay</p>
                  <h2>{{ panelTitle() }}</h2>
                </div>
                <span class="simulation-state">{{
                  snapshot().paused ? "SIMULATION PAUSED" : "SIMULATION LIVE"
                }}</span>
              </header>

              @if (panel() === "pause") {
                <div class="pause-actions">
                  <button type="button" (click)="resumeGame()">Resume</button>
                  <button type="button" (click)="showInventoryFromPause()">
                    Inventory
                  </button>
                  <button type="button" (click)="showSettings()">
                    Settings
                  </button>
                  <button type="button" (click)="openSavePanel()">
                    Save game
                  </button>
                  <button
                    type="button"
                    [disabled]="!loadAvailable()"
                    (click)="openLoadPanel()"
                  >
                    Load game
                  </button>
                  <button
                    type="button"
                    [disabled]="actionBusy() || !snapshot().restartAvailable"
                    (click)="restartGame()"
                  >
                    Restart loading bay
                  </button>
                  <button type="button" class="quiet" (click)="returnToMenu()">
                    Main menu
                  </button>
                </div>
              } @else if (panel() === "save" || panel() === "load") {
                <div class="save-slot-list" [attr.aria-busy]="actionBusy()">
                  @for (slot of snapshot().saveSlots; track slot.slot) {
                    <article
                      class="save-slot"
                      [attr.data-compatibility]="slot.compatibility"
                    >
                      <div>
                        <strong>{{ saveSlotLabel(slot) }}</strong>
                        <span>{{ saveSlotSummary(slot) }}</span>
                        @if (slot.diagnostic !== null) {
                          <small>{{ slot.diagnostic }}</small>
                        }
                      </div>
                      @if (panel() === "save") {
                        @if (overwritePendingSlot() === slot.slot) {
                          <div class="save-slot-confirm">
                            <span>Replace this slot?</span>
                            <button
                              type="button"
                              [disabled]="actionBusy()"
                              (click)="saveToSlot(slot, true)"
                            >
                              Confirm overwrite
                            </button>
                            <button
                              type="button"
                              class="quiet"
                              [disabled]="actionBusy()"
                              (click)="cancelOverwrite()"
                            >
                              Cancel
                            </button>
                          </div>
                        } @else {
                          <button
                            type="button"
                            [disabled]="actionBusy()"
                            (click)="saveToSlot(slot, false)"
                          >
                            {{
                              slot.compatibility === "empty"
                                ? "Save here"
                                : "Overwrite…"
                            }}
                          </button>
                        }
                      } @else {
                        <button
                          type="button"
                          [disabled]="
                            actionBusy() || slot.compatibility !== 'available'
                          "
                          (click)="loadFromSlot(slot)"
                        >
                          Load
                        </button>
                      }
                    </article>
                  }
                </div>
                @if (slotStatus() !== null) {
                  <p class="slot-status" role="status">
                    {{ slotStatus() }}
                  </p>
                }
                <div class="panel-actions">
                  <button
                    type="button"
                    class="quiet"
                    [disabled]="actionBusy()"
                    (click)="showPausePanel()"
                  >
                    Back to pause menu
                  </button>
                </div>
              } @else if (panel() === "inventory") {
                <red-inventory-panel
                  [stacks]="inventoryStacks()"
                  [weapons]="inventoryWeapons()"
                  [capacitySlots]="snapshot().inventoryCapacity"
                  [busy]="actionBusy()"
                  (itemUsed)="useItem($event)"
                  (weaponSelected)="selectWeapon($event)"
                />
                <div class="panel-actions">
                  <button type="button" (click)="closeInventory()">
                    Return to game
                  </button>
                  <button type="button" class="quiet" (click)="openPause()">
                    Pause menu
                  </button>
                </div>
              } @else if (panel() === "settings") {
                <red-settings-panel
                  [settings]="settingsView()"
                  [bindings]="keyBindings()"
                  (sensitivityChanged)="
                    updateSetting('mouseSensitivity', $event)
                  "
                  (invertYChanged)="updateSetting('invertY', $event)"
                  (sfxVolumeChanged)="updateSetting('sfxVolume', $event)"
                  (flashIntensityChanged)="
                    updateSetting('flashIntensity', $event)
                  "
                  (hudVisibleChanged)="updateSetting('hudVisible', $event)"
                  (telemetryVisibleChanged)="
                    updateSetting('telemetryVisible', $event)
                  "
                />
                <div class="panel-actions">
                  <button type="button" (click)="showPausePanel()">Done</button>
                </div>
              }

              @if (snapshot().lastRejection !== null) {
                <p class="action-rejection" role="alert">
                  {{ snapshot().lastRejection }}
                </p>
              }
            </article>
          </section>
        }
      </section>

      <details
        class="diagnostic-drawer"
        [attr.inert]="modalActive() ? '' : null"
      >
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
            <div class="pose-readout" id="inventory-state">
              Awaiting inventory projection
            </div>
            <div class="pose-readout" id="pickup-state">
              Awaiting pickup projection
            </div>
            <div class="pose-readout" id="environment-state">
              Awaiting environment projection
            </div>
            <div class="pose-readout" id="voxel-state">
              Awaiting voxel projection
            </div>
            <p class="telemetry-label">Shared renderer timing</p>
            <div id="renderer-telemetry" class="renderer-telemetry"></div>
            <p class="telemetry-note">
              Cadence is time between submitted frames. Backend submission is
              synchronous host time, not GPU completion.
            </p>
            <p class="telemetry-label">Game session and fixed simulation</p>
            <pre id="session-telemetry" class="session-telemetry">
Awaiting session telemetry</pre
            >
          </article>

          <article class="control-panel">
            <p class="section-label">Player and authoring actions</p>
            <div class="button-row">
              <button type="button" id="primary-fire">Fire Primary</button>
              <button type="button" id="activate-beacon">
                Activate Extraction
              </button>
              <button type="button" id="use-health-supply">
                Use Med Patch
              </button>
              <button type="button" id="remove-voxel">Remove Voxel</button>
              <button type="button" id="place-voxel">Place Voxel</button>
              <button type="button" id="reset" class="quiet">Reset</button>
              <button type="button" class="quiet" disabled>
                Checkpoint unavailable
              </button>
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
  protected readonly settingsRepository = browserHostUserSettingsRepository();
  protected readonly settings = signal(this.settingsRepository.read());
  protected readonly panel = signal<GamePanel>("game");
  protected readonly actionBusy = signal(false);
  protected readonly slotStatus = signal<string | null>(null);
  protected readonly overwritePendingSlot = signal<LoadingBaySaveSlotId | null>(
    null,
  );
  protected readonly connectionState = signal<ConnectionState>("connecting");
  protected readonly connectionMessage = signal(
    "Connecting to the Rust-owned fixed simulation…",
  );
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
  protected readonly hotbarSlots = computed<readonly GameHotbarSlotView[]>(() =>
    this.snapshot().weaponSlots.map((weapon) => ({
      slot: weapon.slot,
      keybind:
        this.snapshot().bindings.selectWeapon[weapon.slot] ??
        String(weapon.slot + 1),
      label: itemLabel(weapon.item),
      owned: weapon.owned,
      selected: weapon.selected,
      ammunition: weapon.ammunitionQuantity,
    })),
  );
  protected readonly carriedKeys = computed(() =>
    this.snapshot()
      .inventoryStacks.filter((stack) => stack.item.startsWith("key/"))
      .map((stack) => stack.item),
  );
  protected readonly checkpointSlot = computed(
    () =>
      this.snapshot().saveSlots.find(
        (slot) =>
          slot.slot === "checkpoint" && slot.compatibility === "available",
      ) ?? null,
  );
  protected readonly loadAvailable = computed(() =>
    this.snapshot().saveSlots.some(
      (slot) => slot.compatibility === "available",
    ),
  );
  protected readonly inventoryStacks = computed<readonly InventoryStackView[]>(
    () =>
      this.snapshot().inventoryStacks.map((stack) => ({
        item: stack.item,
        label: itemLabel(stack.item),
        quantity: stack.quantity,
        category: itemCategory(stack.item),
        usable: stack.item === "supply/med-patch",
      })),
  );
  protected readonly inventoryWeapons = computed<
    readonly InventoryWeaponView[]
  >(() =>
    this.snapshot().weaponSlots.map((weapon) => ({
      slot: weapon.slot,
      label: itemLabel(weapon.item),
      owned: weapon.owned,
      selected: weapon.selected,
      ammunitionLabel: itemLabel(weapon.ammunition),
      ammunitionQuantity: weapon.ammunitionQuantity,
    })),
  );
  protected readonly settingsView = computed<HostSettingsView>(() => ({
    ...this.settings(),
  }));
  protected readonly keyBindings = computed<readonly KeyBindingView[]>(() => {
    const bindings = this.snapshot().bindings;
    return [
      { action: "Move forward", binding: bindings.moveForward },
      { action: "Move backward", binding: bindings.moveBackward },
      { action: "Strafe left", binding: bindings.moveLeft },
      { action: "Strafe right", binding: bindings.moveRight },
      { action: "Look", binding: bindings.mouseLook },
      { action: "Primary fire", binding: bindings.primaryFire },
      ...bindings.selectWeapon.map((binding, index) => ({
        action: `Weapon slot ${String(index + 1)}`,
        binding,
      })),
    ];
  });
  protected readonly modalActive = computed(
    () =>
      this.connectionState() !== "connected" ||
      this.panel() !== "game" ||
      this.snapshot().vitalityState === "dead" ||
      this.snapshot().levelComplete,
  );

  private readonly documentEffects = browserDocumentEffects();
  private readonly route = inject(ActivatedRoute);
  private readonly router = inject(Router);
  private destroyed = false;
  private focusReturnTarget: HTMLElement | null = null;
  private handle: LoadingBayGameHandle | null = null;

  ngAfterViewInit(): void {
    this.documentEffects.setTitle("Rusty Engine — Loading Bay");
    this.documentEffects.setRootClass("game-route-active", true);
    this.scheduleModalFocus();
    void this.mountRuntime();
  }

  ngOnDestroy(): void {
    this.destroyed = true;
    this.documentEffects.setRootClass("game-route-active", false);
    const handle = this.handle;
    this.handle = null;
    delete window.__loadingBayAnimationCapture;
    if (handle !== null) {
      document.body.dataset.rendererLifecycle = "disposed";
      void handle.dispose();
    }
  }

  @HostListener("window:keydown", ["$event"])
  protected onWindowKeydown(event: KeyboardEvent): void {
    if (event.defaultPrevented) {
      return;
    }
    if (event.code === "Tab" && this.modalActive()) {
      this.containModalFocus(event);
      return;
    }
    if (isTextEntry(event.target)) {
      return;
    }
    if (event.code === "Escape") {
      event.preventDefault();
      if (
        this.panel() === "game" &&
        (this.snapshot().vitalityState === "dead" ||
          this.snapshot().levelComplete)
      ) {
        return;
      }
      if (this.panel() === "game") {
        this.openPause();
      } else if (this.panel() === "inventory") {
        this.closeInventory();
      } else if (
        this.panel() === "settings" ||
        this.panel() === "save" ||
        this.panel() === "load"
      ) {
        this.showPausePanel();
      } else {
        this.resumeGame();
      }
      return;
    }
    if (event.code === "KeyI" && this.panel() === "game") {
      event.preventDefault();
      this.openInventory();
      return;
    }
    if (
      event.code === "KeyE" &&
      this.panel() === "game" &&
      this.snapshot().interactionTarget !== null
    ) {
      event.preventDefault();
      this.activateInteraction();
    }
  }

  protected panelTitle(): string {
    switch (this.panel()) {
      case "inventory":
        return "Inventory";
      case "load":
        return "Load game";
      case "pause":
        return "Paused";
      case "save":
        return "Save game";
      case "settings":
        return "Settings";
      case "game":
        return "Game";
    }
  }

  protected itemLabel(item: string): string {
    return itemLabel(item);
  }

  protected openPause(): void {
    this.rememberFocusForModal();
    this.clearSlotOperationState();
    void this.withAction(async (handle) => {
      await handle.setPaused(true);
      this.panel.set("pause");
      this.scheduleModalFocus();
    });
  }

  protected openInventory(): void {
    const handle = this.handle;
    if (handle === null) {
      return;
    }
    this.rememberFocusForModal();
    handle.releaseInput();
    this.panel.set("inventory");
    this.scheduleModalFocus();
  }

  protected showInventoryFromPause(): void {
    void this.withAction(async (handle) => {
      await handle.setPaused(false);
      handle.releaseInput();
      this.panel.set("inventory");
      this.scheduleModalFocus();
    });
  }

  protected closeInventory(): void {
    this.panel.set("game");
    this.restoreModalFocus();
  }

  protected showSettings(): void {
    this.clearSlotOperationState();
    this.panel.set("settings");
    this.scheduleModalFocus();
  }

  protected showPausePanel(): void {
    this.clearSlotOperationState();
    this.panel.set("pause");
    this.scheduleModalFocus();
  }

  protected openSavePanel(): void {
    this.clearSlotOperationState();
    this.panel.set("save");
    this.scheduleModalFocus();
  }

  protected openLoadPanel(): void {
    this.clearSlotOperationState();
    this.panel.set("load");
    this.scheduleModalFocus();
  }

  protected cancelOverwrite(): void {
    this.overwritePendingSlot.set(null);
    this.slotStatus.set("Overwrite cancelled.");
  }

  protected saveSlotLabel(slot: LoadingBaySaveSlot): string {
    return slot.metadata?.displayName ?? slotLabel(slot.slot);
  }

  protected saveSlotSummary(slot: LoadingBaySaveSlot): string {
    const metadata = slot.metadata;
    if (slot.compatibility === "empty") {
      return "Empty slot";
    }
    if (slot.compatibility === "corrupt") {
      return "Corrupt save — overwrite only";
    }
    if (slot.compatibility === "incompatible") {
      return "Incompatible with this authored project";
    }
    if (metadata === null) {
      return "Available save";
    }
    const posture =
      metadata.playerState === "dead"
        ? "player down"
        : metadata.levelComplete
          ? "level complete"
          : "in progress";
    return `Tick ${String(metadata.tick)} · ${posture} · ${formatSaveTime(metadata.savedAtUnixMilliseconds)}`;
  }

  protected saveToSlot(slot: LoadingBaySaveSlot, overwrite: boolean): void {
    if (slot.compatibility !== "empty" && !overwrite) {
      this.overwritePendingSlot.set(slot.slot);
      this.slotStatus.set(
        `${this.saveSlotLabel(slot)} already contains data. Confirm to replace it.`,
      );
      this.scheduleModalFocus();
      return;
    }
    void this.withSlotAction(async (handle) => {
      await handle.saveGame(slot.slot, overwrite, slot.storageRevision);
      this.overwritePendingSlot.set(null);
      this.slotStatus.set(`${this.saveSlotLabel(slot)} saved.`);
    });
  }

  protected loadFromSlot(slot: LoadingBaySaveSlot): void {
    if (slot.compatibility !== "available") {
      return;
    }
    void this.withSlotAction(async (handle) => {
      await handle.loadGame(slot.slot, slot.storageRevision);
      this.panel.set("game");
      this.focusReturnTarget = null;
      this.slotStatus.set(null);
      globalThis.setTimeout(() => this.focusViewport(), 0);
    });
  }

  protected restoreCheckpoint(): void {
    const checkpoint = this.checkpointSlot();
    if (checkpoint !== null) {
      this.loadFromSlot(checkpoint);
    }
  }

  protected resumeGame(): void {
    void this.withAction(async (handle) => {
      if (this.snapshot().paused) {
        await handle.setPaused(false);
      }
      this.panel.set("game");
      this.restoreModalFocus();
    });
  }

  protected selectWeapon(slot: number): void {
    void this.withAction((handle) => handle.selectWeaponSlot(slot));
  }

  protected useItem(item: string): void {
    void this.withAction((handle) => handle.useItem(item));
  }

  protected activateInteraction(): void {
    const target = this.snapshot().interactionTarget;
    if (target !== null) {
      void this.withAction((handle) => handle.interact(target));
    }
  }

  protected restartGame(): void {
    void this.withAction(async (handle) => {
      await handle.restart();
      this.panel.set("game");
      this.focusReturnTarget = null;
      globalThis.setTimeout(() => {
        if (
          !this.destroyed &&
          this.panel() === "game" &&
          this.snapshot().vitalityState !== "dead" &&
          !this.snapshot().levelComplete
        ) {
          this.focusViewport();
        }
      }, 0);
    });
  }

  protected updateSetting<Key extends keyof HostUserSettings>(
    key: Key,
    value: HostUserSettings[Key],
  ): void {
    const next = this.settingsRepository.write({
      ...this.settings(),
      [key]: value,
    });
    this.settings.set(next);
    this.handle?.updatePreferences(runtimePreferences(next));
  }

  protected retryConnection(): void {
    void (async () => {
      this.connectionState.set("reconnecting");
      this.connectionMessage.set("Reconnecting to the game host…");
      const handle = this.handle;
      this.handle = null;
      if (handle !== null) {
        await handle.dispose();
      }
      await this.mountRuntime();
    })();
  }

  protected returnToMenu(): void {
    void (async () => {
      const handle = this.handle;
      if (
        handle !== null &&
        this.connectionState() === "connected" &&
        !this.snapshot().paused
      ) {
        try {
          await handle.setPaused(true);
        } catch {
          // Navigation disposes and disconnects the failed input session.
        }
      }
      await this.router.navigateByUrl("/");
    })();
  }

  private async mountRuntime(): Promise<void> {
    if (this.destroyed) {
      return;
    }
    document.body.dataset.rendererLifecycle = "mounting";
    try {
      const handle = await mountLoadingBayGame({
        onProjection: (snapshot) => {
          const wasDead = this.snapshot().vitalityState === "dead";
          const wasComplete = this.snapshot().levelComplete;
          this.snapshot.set(snapshot);
          if (
            (!wasDead && snapshot.vitalityState === "dead") ||
            (!wasComplete && snapshot.levelComplete)
          ) {
            this.rememberFocusForModal();
            this.scheduleModalFocus();
          }
        },
        onConnectionFailure: (message) => {
          this.connectionState.set("unavailable");
          this.connectionMessage.set(message);
          this.rememberFocusForModal();
          this.scheduleModalFocus();
        },
        preferences: runtimePreferences(this.settings()),
      });
      if (this.destroyed) {
        await handle.dispose();
        return;
      }
      this.handle = handle;
      if (this.route.snapshot.queryParamMap.get("visualQa") === "animation") {
        window.__loadingBayAnimationCapture = handle.captureAnimation;
      }
      const requestedProject = this.route.snapshot.queryParamMap.get("project");
      if (requestedProject !== null && !["loading-bay", "relay-annex", "doom-e1m1"].includes(requestedProject)) {
        throw new Error(`Unknown project ${requestedProject}`);
      }
      // Host is launched with a fixed --project (see main-menu startScene). The URL param is retained
      // for deep-linking and for tests to assert that a click on the Doom card produces
      // `project=doom-e1m1` and that the authoritative host/session reflects that scene.
      // If URL and host disagree, the host wins – the UI will show the host's actual scene
      // after the restart, and a mismatch is observable via /api/state.
      const entryMode = this.route.snapshot.queryParamMap.get("mode");
      if (entryMode === "new") {
        if (this.snapshot().paused) {
          await handle.setPaused(false);
        }
        await handle.restart();
      } else if (entryMode === "load") {
        const slot = parseSaveSlotId(
          this.route.snapshot.queryParamMap.get("slot"),
        );
        const storageRevision =
          this.route.snapshot.queryParamMap.get("revision");
        if (slot === null || storageRevision === null) {
          throw new Error("The selected save slot is no longer available.");
        }
        await handle.loadGame(slot, storageRevision);
      } else if (entryMode === "continue") {
        if (
          !this.settingsRepository.hasContinueSession(
            this.snapshot().hostSessionId,
          )
        ) {
          await handle.dispose();
          this.handle = null;
          throw new Error(
            "That Rust host session is no longer available. Start a new game.",
          );
        }
        await handle.setPaused(false);
      }
      this.settingsRepository.markContinueSessionAvailable(
        this.snapshot().hostSessionId,
      );
      this.connectionState.set("connected");
      this.connectionMessage.set("Connected");
      if (
        this.panel() === "game" &&
        this.snapshot().vitalityState !== "dead" &&
        !this.snapshot().levelComplete
      ) {
        this.restoreModalFocus();
      }
      document.body.dataset.rendererLifecycle = "mounted";
      if (new URLSearchParams(location.search).has("lifecycle-smoke")) {
        await this.router.navigateByUrl("/diagnostics");
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.connectionState.set("unavailable");
      this.connectionMessage.set(message);
      this.rememberFocusForModal();
      this.scheduleModalFocus();
      document.body.dataset.rendererLifecycle = "failed";
      document.body.dataset.runtimeError = message;
    }
  }

  private async withAction(
    operation: (handle: LoadingBayGameHandle) => Promise<void>,
  ): Promise<void> {
    const handle = this.handle;
    if (handle === null || this.actionBusy()) {
      return;
    }
    this.actionBusy.set(true);
    try {
      await operation(handle);
    } catch {
      // The runtime publishes the typed rejection through the projection.
    } finally {
      this.actionBusy.set(false);
    }
  }

  private async withSlotAction(
    operation: (handle: LoadingBayGameHandle) => Promise<void>,
  ): Promise<void> {
    const handle = this.handle;
    if (handle === null || this.actionBusy()) {
      return;
    }
    this.actionBusy.set(true);
    this.slotStatus.set("Waiting for the Rust-owned fixed tick…");
    try {
      await operation(handle);
    } catch (error) {
      this.slotStatus.set(
        error instanceof Error ? error.message : String(error),
      );
    } finally {
      this.actionBusy.set(false);
    }
  }

  private clearSlotOperationState(): void {
    this.overwritePendingSlot.set(null);
    this.slotStatus.set(null);
  }

  private focusViewport(): void {
    document.getElementById("viewport")?.focus();
  }

  private rememberFocusForModal(): void {
    if (this.focusReturnTarget !== null) {
      return;
    }
    const active = document.activeElement;
    this.focusReturnTarget = active instanceof HTMLElement ? active : null;
  }

  private scheduleModalFocus(): void {
    globalThis.setTimeout(() => {
      if (this.destroyed || !this.modalActive()) {
        return;
      }
      const modal = document.querySelector<HTMLElement>("[data-active-modal]");
      if (modal === null) {
        return;
      }
      const first = modal.querySelector<HTMLElement>(FOCUSABLE_SELECTOR);
      (first ?? modal).focus();
    }, 0);
  }

  private containModalFocus(event: KeyboardEvent): void {
    const modal = document.querySelector<HTMLElement>("[data-active-modal]");
    if (modal === null) {
      return;
    }
    const focusable = Array.from(
      modal.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR),
    );
    if (focusable.length === 0) {
      event.preventDefault();
      modal.focus();
      return;
    }
    const active = document.activeElement;
    const index =
      active instanceof HTMLElement ? focusable.indexOf(active) : -1;
    if (index === -1) {
      event.preventDefault();
      focusable[event.shiftKey ? focusable.length - 1 : 0]?.focus();
      return;
    }
    if (!event.shiftKey && index === focusable.length - 1) {
      event.preventDefault();
      focusable[0]?.focus();
    } else if (event.shiftKey && index === 0) {
      event.preventDefault();
      focusable.at(-1)?.focus();
    }
  }

  private restoreModalFocus(): void {
    queueMicrotask(() => {
      const target = this.focusReturnTarget;
      this.focusReturnTarget = null;
      if (target?.isConnected) {
        target.focus();
      } else {
        this.focusViewport();
      }
    });
  }
}

const FOCUSABLE_SELECTOR =
  'button:not([disabled]), a[href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

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

function itemLabel(item: string): string {
  const localName = item.split("/").at(-1) ?? item;
  return localName
    .split("-")
    .map((part) =>
      part.length === 0 ? part : part[0]?.toUpperCase() + part.slice(1),
    )
    .join(" ");
}

function slotLabel(slot: LoadingBaySaveSlotId): string {
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

function parseSaveSlotId(value: string | null): LoadingBaySaveSlotId | null {
  return value === "checkpoint" ||
    value === "slot1" ||
    value === "slot2" ||
    value === "slot3"
    ? value
    : null;
}

function formatSaveTime(unixMilliseconds: number): string {
  try {
    return new Intl.DateTimeFormat(undefined, {
      dateStyle: "medium",
      timeStyle: "short",
    }).format(new Date(unixMilliseconds));
  } catch {
    return "unknown time";
  }
}

function itemCategory(item: string): InventoryStackView["category"] {
  if (item.startsWith("ammo/")) {
    return "ammunition";
  }
  if (item.startsWith("armor/")) {
    return "armor";
  }
  if (item.startsWith("key/")) {
    return "key";
  }
  if (item.startsWith("supply/")) {
    return "supply";
  }
  return "weapon";
}

function runtimePreferences(
  settings: HostUserSettings,
): Parameters<LoadingBayGameHandle["updatePreferences"]>[0] {
  return {
    mouseSensitivity: settings.mouseSensitivity,
    invertY: settings.invertY,
    sfxVolume: settings.sfxVolume,
    flashIntensity: settings.flashIntensity,
    telemetryVisible: settings.telemetryVisible,
  };
}

function isTextEntry(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement
  );
}

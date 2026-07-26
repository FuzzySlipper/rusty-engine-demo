import { ChangeDetectionStrategy, Component, signal } from "@angular/core";
import { RouterLink } from "@angular/router";
import {
  browserDocumentEffects,
  browserHostUserSettingsRepository,
  type HostUserSettings,
} from "@rusty-engine-demo/platform";
import {
  SettingsPanelComponent,
  type HostSettingsView,
} from "@rusty-engine-demo/ui-game-panels";

@Component({
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, SettingsPanelComponent],
  selector: "red-settings-screen",
  standalone: true,
  template: `
    <main class="settings-screen">
      <section class="settings-card">
        <header>
          <div>
            <p class="section-label">Host-user preferences</p>
            <h1>Settings</h1>
          </div>
          <a routerLink="/">Back to main menu</a>
        </header>
        <red-settings-panel
          [settings]="settingsView()"
          [bindings]="[]"
          (sensitivityChanged)="update('mouseSensitivity', $event)"
          (invertYChanged)="update('invertY', $event)"
          (sfxVolumeChanged)="update('sfxVolume', $event)"
          (hudVisibleChanged)="update('hudVisible', $event)"
          (telemetryVisibleChanged)="update('telemetryVisible', $event)"
        />
        <p class="settings-saved" aria-live="polite">
          {{ saveStatus() }}
        </p>
      </section>
    </main>
  `,
})
export class SettingsScreenComponent {
  protected readonly settingsView = signal<HostSettingsView>(
    browserHostUserSettingsRepository().read(),
  );
  protected readonly saveStatus = signal("Preferences load from this browser.");

  private readonly documentEffects = browserDocumentEffects();
  private readonly repository = browserHostUserSettingsRepository();

  constructor() {
    this.documentEffects.setTitle("Loading Bay — Settings");
  }

  protected update<Key extends keyof HostUserSettings>(
    key: Key,
    value: HostUserSettings[Key],
  ): void {
    const next = this.repository.write({
      ...this.settingsView(),
      [key]: value,
    });
    this.settingsView.set(next);
    this.saveStatus.set("Saved for this browser.");
  }
}

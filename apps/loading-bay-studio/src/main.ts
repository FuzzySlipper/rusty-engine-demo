import { provideZonelessChangeDetection } from "@angular/core";
import { bootstrapApplication } from "@angular/platform-browser";
import { StudioAdapterClient } from "@rusty-engine/studio-adapter-client";
import {
  HttpStudioAdapterTransport,
  STUDIO_WORKSPACE,
  StudioWorkspaceStore,
} from "@rusty-engine/studio-editor-shell";
import { HttpStudioUserSettingsClient } from "@rusty-engine/studio-user-settings";
import {
  HttpLoadingBayWeaponAuthoringPort,
  LOADING_BAY_WEAPON_AUTHORING_CLIENT,
  LoadingBayWeaponAuthoringClient,
} from "@rusty-engine-demo/studio-weapon-inspector";

import { LoadingBayStudioAppComponent } from "./app.component.js";

void bootstrapApplication(LoadingBayStudioAppComponent, {
  providers: [
    provideZonelessChangeDetection(),
    {
      provide: STUDIO_WORKSPACE,
      useFactory: () =>
        new StudioWorkspaceStore(
          new StudioAdapterClient(new HttpStudioAdapterTransport()),
          new HttpStudioUserSettingsClient(),
        ),
    },
    {
      provide: LOADING_BAY_WEAPON_AUTHORING_CLIENT,
      useFactory: () =>
        new LoadingBayWeaponAuthoringClient(
          new HttpLoadingBayWeaponAuthoringPort(),
          () => globalThis.crypto.randomUUID(),
        ),
    },
  ],
});

import {
  ChangeDetectionStrategy,
  Component,
  inject,
  type OnInit,
} from "@angular/core";
import {
  RUSTY_ENGINE_ENTITY_INSPECTOR_CONTRIBUTIONS,
  STUDIO_WORKSPACE,
  StudioShellComponent,
} from "@rusty-engine/studio-editor-shell";
import { LOADING_BAY_WEAPON_INSPECTOR_CONTRIBUTION } from "@rusty-engine-demo/studio-weapon-inspector";

import { readStartupProject } from "./studio-startup.js";

@Component({
  selector: "loading-bay-studio-root",
  standalone: true,
  imports: [StudioShellComponent],
  template: `
    <rusty-studio-shell
      [entityInspectorContributions]="entityInspectorContributions"
    />
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class LoadingBayStudioAppComponent implements OnInit {
  readonly entityInspectorContributions = Object.freeze([
    ...RUSTY_ENGINE_ENTITY_INSPECTOR_CONTRIBUTIONS,
    LOADING_BAY_WEAPON_INSPECTOR_CONTRIBUTION,
  ]);
  readonly #workspace = inject(STUDIO_WORKSPACE);

  ngOnInit(): void {
    const startup = readStartupProject(globalThis.location?.href ?? "");
    if (startup === null) {
      void this.#workspace.connect();
      return;
    }
    if ("diagnostic" in startup) {
      void this.#workspace.connect();
      this.#workspace.reportUiError(startup.diagnostic);
      return;
    }
    void this.#workspace.openProject(startup.root, startup.projectFile);
  }
}

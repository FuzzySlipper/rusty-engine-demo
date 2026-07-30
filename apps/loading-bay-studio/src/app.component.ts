import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  signal,
  type OnInit,
} from "@angular/core";
import {
  RUSTY_ENGINE_ENTITY_INSPECTOR_CONTRIBUTIONS,
  STUDIO_WORKSPACE,
  StudioShellComponent,
} from "@rusty-engine/studio-editor-shell";
import type { StudioViewportFrameSubmitted } from "@rusty-engine/studio-viewport";
import { LOADING_BAY_WEAPON_INSPECTOR_CONTRIBUTION } from "@rusty-engine-demo/studio-weapon-inspector";

import {
  appendStudioFrameSubmission,
  studioFrameSubmissionEvidence,
} from "./studio-frame-submission.js";
import { readStartupProject } from "./studio-startup.js";

@Component({
  selector: "loading-bay-studio-root",
  standalone: true,
  imports: [StudioShellComponent],
  host: {
    "[attr.data-frame-submission-evidence]": "frameSubmissionEvidenceJson()",
  },
  template: `
    <rusty-studio-shell
      [entityInspectorContributions]="entityInspectorContributions"
      (frameSubmitted)="recordFrameSubmission($event)"
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
  readonly #frameSubmissionCount = signal(0);
  readonly #frameSubmissions = signal<readonly StudioViewportFrameSubmitted[]>(
    [],
  );
  readonly frameSubmissionEvidenceJson = computed(() =>
    JSON.stringify(
      studioFrameSubmissionEvidence(
        this.#frameSubmissionCount(),
        this.#frameSubmissions(),
      ),
    ),
  );

  recordFrameSubmission(event: StudioViewportFrameSubmitted): void {
    this.#frameSubmissionCount.update((count) => count + 1);
    this.#frameSubmissions.update((history) =>
      appendStudioFrameSubmission(history, event),
    );
  }

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

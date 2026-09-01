import { InjectionToken } from "@angular/core";
import type { RustyApplicationUiContext } from "@rusty-engine/application-host";

/** The read-only/claim-only Engine UI context mounted by Product Browser Host. */
export const ENGINE_APPLICATION = new InjectionToken<RustyApplicationUiContext>(
  "ENGINE_APPLICATION",
);

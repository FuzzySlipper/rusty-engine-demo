import { InjectionToken } from "@angular/core";
import type { RustyApplicationUiContext } from "@rusty-engine/application-host";

/** The sole public Engine web-host seam composed by this product bootstrap. */
export const ENGINE_APPLICATION = new InjectionToken<RustyApplicationUiContext>(
  "ENGINE_APPLICATION",
);

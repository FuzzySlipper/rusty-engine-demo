import { InjectionToken } from "@angular/core";

/** The runtime shell supplies this read-only projection port to the product UI. */
export interface LoadingBayEngineApplication {
  readonly projection?: {
    subscribe(
      listener: (envelope: LoadingBayHudProjectionEnvelope | null) => void,
    ): () => void;
  };
}

export interface LoadingBayHudProjectionEnvelope {
  readonly stream: string;
  readonly contract: string;
  readonly value: unknown;
}

export const ENGINE_APPLICATION = new InjectionToken<LoadingBayEngineApplication>(
  "ENGINE_APPLICATION",
);

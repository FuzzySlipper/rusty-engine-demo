export interface DocumentEffectsPort {
  readonly setRootClass: (className: string, enabled: boolean) => void;
  readonly setTitle: (title: string) => void;
}

export const browserDocumentEffects = (): DocumentEffectsPort => ({
  setRootClass: (className, enabled) => {
    document.documentElement.classList.toggle(className, enabled);
  },
  setTitle: (title) => {
    document.title = title;
  },
});

import type {
  RuntimeAnimationState,
  RuntimeBrowserState,
  RuntimeFeedbackCue,
} from "./projection.js";

export type FeedbackParticleKind =
  | "movement"
  | "blocked"
  | "muzzle"
  | "impact"
  | "defeat"
  | "door";
export type FeedbackSoundKind =
  | "step"
  | "blocked"
  | "shot"
  | "hit"
  | "defeat"
  | "doorOpen"
  | "doorClose";

export interface FeedbackAnchor {
  readonly entity: number;
  readonly position: readonly [number, number, number];
}

export interface PresentationFeedbackSink {
  clearTransient(): void;
  setAnimationState(state: RuntimeAnimationState): void;
  pulseAnimation(entity: number, name: string): void;
  emitParticle(kind: FeedbackParticleKind, anchor: FeedbackAnchor): void;
  showBillboard(text: string, tone: "neutral" | "warning" | "success", anchor: FeedbackAnchor): void;
  playSound(kind: FeedbackSoundKind): boolean;
  activateAudio(): Promise<"running" | "blocked" | "unavailable">;
}

export interface FeedbackApplicationReceipt {
  readonly cueCount: number;
  readonly failedOperations: number;
  readonly scheduledSounds: number;
}

/**
 * Direct semantic-cue to browser-feedback mapping. This adapter has no input,
 * gameplay, or persistence methods; sink failures are counted and discarded.
 */
export class PresentationFeedbackAdapter {
  readonly #sink: PresentationFeedbackSink;

  constructor(sink: PresentationFeedbackSink) {
    this.#sink = sink;
  }

  async activateAudio(): Promise<"running" | "blocked" | "unavailable"> {
    try {
      return await this.#sink.activateAudio();
    } catch {
      return "blocked";
    }
  }

  apply(state: RuntimeBrowserState, reset = false): FeedbackApplicationReceipt {
    let failedOperations = 0;
    let scheduledSounds = 0;
    const attempt = (operation: () => void): void => {
      try {
        operation();
      } catch {
        failedOperations += 1;
      }
    };
    const sound = (kind: FeedbackSoundKind): void => {
      attempt(() => {
        if (this.#sink.playSound(kind)) {
          scheduledSounds += 1;
        }
      });
    };

    if (reset) {
      attempt(() => this.#sink.clearTransient());
    }
    for (const animation of state.presentation.animationStates) {
      attempt(() => this.#sink.setAnimationState(animation));
    }
    for (const cue of state.presentation.cues) {
      const anchor = cueAnchor(state, cue);
      switch (cue.kind) {
        case "movement":
          attempt(() => this.#sink.pulseAnimation(cue.entity, "movement"));
          attempt(() => this.#sink.emitParticle("movement", anchor));
          sound("step");
          break;
        case "movementBlocked":
          attempt(() => this.#sink.pulseAnimation(cue.entity, "blocked"));
          attempt(() => this.#sink.emitParticle("blocked", anchor));
          attempt(() => this.#sink.showBillboard("BLOCKED", "warning", anchor));
          sound("blocked");
          break;
        case "attack":
          attempt(() => this.#sink.pulseAnimation(cue.attacker, "attack"));
          attempt(() => this.#sink.emitParticle("muzzle", anchor));
          sound("shot");
          break;
        case "damage":
          attempt(() => this.#sink.pulseAnimation(cue.target, "damage"));
          attempt(() => this.#sink.emitParticle("impact", anchor));
          attempt(() => this.#sink.showBillboard(`-${String(cue.amount)}`, "warning", anchor));
          sound("hit");
          break;
        case "defeat":
          attempt(() => this.#sink.pulseAnimation(cue.entity, "defeat"));
          attempt(() => this.#sink.emitParticle("defeat", anchor));
          attempt(() => this.#sink.showBillboard("DEFEATED", "neutral", anchor));
          sound("defeat");
          break;
        case "doorChanged":
          attempt(() => this.#sink.pulseAnimation(cue.entity, cue.state));
          attempt(() => this.#sink.emitParticle("door", anchor));
          attempt(() => this.#sink.showBillboard(
            cue.state === "open" ? "EXIT OPEN" : "EXIT SEALED",
            cue.state === "open" ? "success" : "neutral",
            anchor,
          ));
          sound(cue.state === "open" ? "doorOpen" : "doorClose");
          break;
      }
    }
    return {
      cueCount: state.presentation.cues.length,
      failedOperations,
      scheduledSounds,
    };
  }
}

function cueAnchor(state: RuntimeBrowserState, cue: RuntimeFeedbackCue): FeedbackAnchor {
  switch (cue.kind) {
    case "movement":
      return { entity: cue.entity, position: cue.to };
    case "attack":
      return { entity: cue.attacker, position: cue.origin };
    case "movementBlocked":
      return entityAnchor(state, cue.entity);
    case "damage":
      return entityAnchor(state, cue.target);
    case "defeat":
      return entityAnchor(state, cue.entity);
    case "doorChanged":
      return entityAnchor(state, cue.entity);
  }
}

function entityAnchor(state: RuntimeBrowserState, entity: number): FeedbackAnchor {
  if (state.player.id === entity) {
    return { entity, position: state.player.position };
  }
  const enemy = state.enemies.find((candidate) => candidate.id === entity);
  if (enemy !== undefined) {
    return { entity, position: enemy.position };
  }
  const node = state.projection.find((candidate) => candidate.id === entity);
  return { entity, position: node?.translation ?? [0, 0, 0] };
}

const MAX_ACTIVE_EFFECTS = 24;
const PARTICLE_LIFETIME_MILLISECONDS = 700;
const BILLBOARD_LIFETIME_MILLISECONDS = 1_100;
const PULSE_LIFETIME_MILLISECONDS = 420;

export interface BrowserPresentationHost {
  queryEntity(entity: number): HTMLElement | null;
  createElement(tagName: "span" | "strong"): HTMLElement;
  createAudioContext(): AudioContext | null;
  setTimeout(
    operation: () => void,
    delayMilliseconds: number,
  ): ReturnType<typeof globalThis.setTimeout>;
  clearTimeout(timeout: ReturnType<typeof globalThis.setTimeout>): void;
}

const DEFAULT_BROWSER_PRESENTATION_HOST: BrowserPresentationHost = {
  queryEntity(entity) {
    return document.querySelector<HTMLElement>(`[data-entity-id="${String(entity)}"]`);
  },
  createElement(tagName) {
    return document.createElement(tagName);
  },
  createAudioContext() {
    const Context = globalThis.AudioContext;
    return Context === undefined ? null : new Context();
  },
  setTimeout(operation, delayMilliseconds) {
    return globalThis.setTimeout(operation, delayMilliseconds);
  },
  clearTimeout(timeout) {
    globalThis.clearTimeout(timeout);
  },
};

interface ActiveSound {
  readonly oscillator: OscillatorNode;
  readonly gain: GainNode;
}

/** DOM/Web Audio realization for the concrete browser shell. */
export class BrowserPresentationFeedbackSink implements PresentationFeedbackSink {
  readonly #layer: HTMLElement;
  readonly #audioStatus: HTMLElement;
  readonly #host: BrowserPresentationHost;
  readonly #active: HTMLElement[] = [];
  readonly #pulseTargets = new Set<HTMLElement>();
  readonly #activeSounds = new Set<ActiveSound>();
  readonly #timeouts = new Set<ReturnType<typeof globalThis.setTimeout>>();
  #audioContext: AudioContext | null = null;
  #soundAttempts = 0;
  #scheduledSounds = 0;
  #droppedSounds = 0;

  constructor(
    layer: HTMLElement,
    audioStatus: HTMLElement,
    host: BrowserPresentationHost = DEFAULT_BROWSER_PRESENTATION_HOST,
  ) {
    this.#layer = layer;
    this.#audioStatus = audioStatus;
    this.#host = host;
    this.#setAudioStatus("inactive");
    this.#audioStatus.dataset.activeSounds = "0";
  }

  async activateAudio(): Promise<"running" | "blocked" | "unavailable"> {
    try {
      if (this.#audioContext === null) {
        this.#audioContext = this.#host.createAudioContext();
      }
      if (this.#audioContext === null) {
        this.#setAudioStatus("unavailable");
        return "unavailable";
      }
      if (this.#audioContext.state === "suspended") {
        await this.#audioContext.resume();
      }
      const state = this.#audioContext.state === "running" ? "running" : "blocked";
      this.#setAudioStatus(state);
      return state;
    } catch {
      this.#setAudioStatus("blocked");
      return "blocked";
    }
  }

  clearTransient(): void {
    for (const timeout of this.#timeouts) {
      this.#host.clearTimeout(timeout);
    }
    this.#timeouts.clear();
    for (const target of this.#pulseTargets) {
      delete target.dataset.animationPulse;
    }
    this.#pulseTargets.clear();
    for (const element of this.#active) {
      element.remove();
    }
    this.#active.length = 0;
    for (const sound of this.#activeSounds) {
      try {
        sound.oscillator.stop();
      } catch {
        // A source that ended between dispatch and reset is already silent.
      }
      sound.oscillator.disconnect();
      sound.gain.disconnect();
    }
    this.#activeSounds.clear();
    this.#audioStatus.dataset.activeSounds = "0";
    for (const key of [
      "animationStates",
      "cueKinds",
      "particleKinds",
      "billboardValues",
      "animationPulses",
    ] as const) {
      delete this.#layer.dataset[key];
    }
    this.#layer.dataset.activeEffects = "0";
  }

  setAnimationState(state: RuntimeAnimationState): void {
    const entity = this.#host.queryEntity(state.entity);
    if (entity !== null) {
      entity.dataset.posture = state.posture;
    }
    this.#record("animationStates", `${String(state.entity)}:${state.posture}`);
  }

  pulseAnimation(entity: number, name: string): void {
    const target = this.#host.queryEntity(entity);
    if (target !== null) {
      target.dataset.animationPulse = name;
      this.#pulseTargets.add(target);
      this.#schedule(() => {
        if (target.dataset.animationPulse === name) {
          delete target.dataset.animationPulse;
          this.#pulseTargets.delete(target);
        }
      }, PULSE_LIFETIME_MILLISECONDS);
    }
    this.#record("animationPulses", name);
    this.#record("cueKinds", name);
  }

  emitParticle(kind: FeedbackParticleKind, anchor: FeedbackAnchor): void {
    const element = this.#host.createElement("span");
    element.className = `feedback-particle feedback-particle--${kind}`;
    element.dataset.kind = kind;
    this.#position(element, anchor.position);
    this.#appendTransient(element, PARTICLE_LIFETIME_MILLISECONDS);
    this.#record("particleKinds", kind);
  }

  showBillboard(
    text: string,
    tone: "neutral" | "warning" | "success",
    anchor: FeedbackAnchor,
  ): void {
    const element = this.#host.createElement("strong");
    element.className = "feedback-billboard";
    element.dataset.tone = tone;
    element.dataset.entityId = String(anchor.entity);
    element.textContent = text;
    this.#position(element, anchor.position);
    this.#appendTransient(element, BILLBOARD_LIFETIME_MILLISECONDS);
    this.#record("billboardValues", text);
  }

  playSound(kind: FeedbackSoundKind): boolean {
    this.#soundAttempts += 1;
    this.#audioStatus.dataset.attempted = String(this.#soundAttempts);
    const context = this.#audioContext;
    if (context === null || context.state !== "running") {
      this.#droppedSounds += 1;
      this.#audioStatus.dataset.dropped = String(this.#droppedSounds);
      return false;
    }
    const profile = SOUND_PROFILES[kind];
    const oscillator = context.createOscillator();
    const gain = context.createGain();
    const start = context.currentTime;
    oscillator.type = profile.wave;
    oscillator.frequency.setValueAtTime(profile.frequency, start);
    oscillator.frequency.exponentialRampToValueAtTime(profile.frequencyEnd, start + profile.duration);
    gain.gain.setValueAtTime(profile.gain, start);
    gain.gain.linearRampToValueAtTime(0, start + profile.duration);
    oscillator.connect(gain);
    gain.connect(context.destination);
    const activeSound = { oscillator, gain };
    this.#activeSounds.add(activeSound);
    this.#audioStatus.dataset.activeSounds = String(this.#activeSounds.size);
    oscillator.addEventListener("ended", () => {
      if (this.#activeSounds.delete(activeSound)) {
        oscillator.disconnect();
        gain.disconnect();
        this.#audioStatus.dataset.activeSounds = String(this.#activeSounds.size);
      }
    }, { once: true });
    oscillator.start(start);
    oscillator.stop(start + profile.duration);
    this.#scheduledSounds += 1;
    this.#audioStatus.dataset.scheduled = String(this.#scheduledSounds);
    this.#audioStatus.dataset.lastSound = kind;
    return true;
  }

  #position(element: HTMLElement, position: readonly [number, number, number]): void {
    const left = clamp(12 + (position[0] / 12) * 76, 8, 92);
    const top = clamp(84 - (position[2] / 15) * 64 - position[1] * 1.5, 12, 86);
    element.style.setProperty("--feedback-left", `${left.toFixed(2)}%`);
    element.style.setProperty("--feedback-top", `${top.toFixed(2)}%`);
  }

  #appendTransient(element: HTMLElement, lifetime: number): void {
    while (this.#active.length >= MAX_ACTIVE_EFFECTS) {
      this.#active.shift()?.remove();
    }
    this.#active.push(element);
    this.#layer.append(element);
    this.#layer.dataset.activeEffects = String(this.#active.length);
    this.#layer.dataset.maxActiveEffects = String(MAX_ACTIVE_EFFECTS);
    this.#schedule(() => {
      const index = this.#active.indexOf(element);
      if (index >= 0) {
        this.#active.splice(index, 1);
      }
      element.remove();
      this.#layer.dataset.activeEffects = String(this.#active.length);
    }, lifetime);
  }

  #schedule(operation: () => void, delay: number): void {
    const timeout = this.#host.setTimeout(() => {
      this.#timeouts.delete(timeout);
      operation();
    }, delay);
    this.#timeouts.add(timeout);
  }

  #record(field: "animationStates" | "animationPulses" | "cueKinds" | "particleKinds" | "billboardValues", value: string): void {
    const values = new Set((this.#layer.dataset[field] ?? "").split(",").filter(Boolean));
    values.add(value);
    this.#layer.dataset[field] = [...values].join(",");
  }

  #setAudioStatus(status: "inactive" | "running" | "blocked" | "unavailable"): void {
    this.#audioStatus.dataset.state = status;
    this.#audioStatus.textContent = status === "running"
      ? "AUDIO ARMED"
      : status === "inactive"
        ? "AUDIO WAITING"
        : status === "unavailable"
          ? "AUDIO UNAVAILABLE"
          : "AUDIO BLOCKED";
  }
}

const SOUND_PROFILES: Record<FeedbackSoundKind, {
  readonly frequency: number;
  readonly frequencyEnd: number;
  readonly duration: number;
  readonly gain: number;
  readonly wave: OscillatorType;
}> = {
  step: { frequency: 95, frequencyEnd: 70, duration: 0.05, gain: 0.025, wave: "triangle" },
  blocked: { frequency: 120, frequencyEnd: 55, duration: 0.11, gain: 0.04, wave: "square" },
  shot: { frequency: 220, frequencyEnd: 48, duration: 0.13, gain: 0.055, wave: "sawtooth" },
  hit: { frequency: 440, frequencyEnd: 180, duration: 0.09, gain: 0.04, wave: "square" },
  defeat: { frequency: 180, frequencyEnd: 48, duration: 0.3, gain: 0.045, wave: "sawtooth" },
  doorOpen: { frequency: 150, frequencyEnd: 310, duration: 0.24, gain: 0.035, wave: "triangle" },
  doorClose: { frequency: 260, frequencyEnd: 90, duration: 0.2, gain: 0.035, wave: "triangle" },
};

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.max(minimum, Math.min(maximum, value));
}

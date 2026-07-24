import assert from "node:assert/strict";
import test from "node:test";

import {
  BrowserPresentationFeedbackSink,
  PresentationFeedbackAdapter,
  type BrowserPresentationHost,
  type FeedbackAnchor,
  type FeedbackParticleKind,
  type FeedbackSoundKind,
  type PresentationFeedbackSink,
} from "./presentation-feedback.ts";
import type { RuntimeAnimationState, RuntimeBrowserState } from "./projection.ts";

class RecordingSink implements PresentationFeedbackSink {
  readonly animations: RuntimeAnimationState[] = [];
  readonly pulses: { readonly entity: number; readonly name: string }[] = [];
  readonly particles: { readonly kind: FeedbackParticleKind; readonly anchor: FeedbackAnchor }[] = [];
  readonly billboards: { readonly text: string; readonly anchor: FeedbackAnchor }[] = [];
  readonly sounds: FeedbackSoundKind[] = [];
  clears = 0;
  failSound: FeedbackSoundKind | null = null;

  clearTransient(): void {
    this.clears += 1;
  }

  setAnimationState(state: RuntimeAnimationState): void {
    this.animations.push(state);
  }

  pulseAnimation(entity: number, name: string): void {
    this.pulses.push({ entity, name });
  }

  emitParticle(kind: FeedbackParticleKind, anchor: FeedbackAnchor): void {
    this.particles.push({ kind, anchor });
  }

  showBillboard(text: string, _tone: "neutral" | "warning" | "success", anchor: FeedbackAnchor): void {
    this.billboards.push({ text, anchor });
  }

  playSound(kind: FeedbackSoundKind): boolean {
    if (kind === this.failSound) {
      throw new Error("simulated audio host failure");
    }
    this.sounds.push(kind);
    return true;
  }

  activateAudio(): Promise<"running"> {
    return Promise.resolve("running");
  }
}

class FakeElement {
  readonly dataset = {} as DOMStringMap;
  readonly children: FakeElement[] = [];
  readonly properties = new Map<string, string>();
  readonly style = {
    setProperty: (name: string, value: string) => {
      this.properties.set(name, value);
    },
  } as unknown as CSSStyleDeclaration;
  className = "";
  textContent: string | null = null;
  removed = false;

  append(...nodes: Node[]): void {
    this.children.push(...nodes.map((node) => node as unknown as FakeElement));
  }

  remove(): void {
    this.removed = true;
  }

  asHtml(): HTMLElement {
    return this as unknown as HTMLElement;
  }
}

class FakeAudioParam {
  setValueAtTime(): void {}
  exponentialRampToValueAtTime(): void {}
  linearRampToValueAtTime(): void {}
}

class FakeOscillator {
  readonly frequency = new FakeAudioParam();
  type: OscillatorType = "sine";
  stopCalls = 0;
  disconnectCalls = 0;

  connect(): void {}
  start(): void {}

  stop(): void {
    this.stopCalls += 1;
  }

  disconnect(): void {
    this.disconnectCalls += 1;
  }

  addEventListener(): void {}

  asNode(): OscillatorNode {
    return this as unknown as OscillatorNode;
  }
}

class FakeGain {
  readonly gain = new FakeAudioParam();
  disconnectCalls = 0;

  connect(): void {}

  disconnect(): void {
    this.disconnectCalls += 1;
  }

  asNode(): GainNode {
    return this as unknown as GainNode;
  }
}

class FakeAudioContext {
  readonly destination = {} as AudioDestinationNode;
  readonly oscillators: FakeOscillator[] = [];
  readonly gains: FakeGain[] = [];
  readonly currentTime = 10;
  readonly state: AudioContextState = "running";

  createOscillator(): OscillatorNode {
    const oscillator = new FakeOscillator();
    this.oscillators.push(oscillator);
    return oscillator.asNode();
  }

  createGain(): GainNode {
    const gain = new FakeGain();
    this.gains.push(gain);
    return gain.asNode();
  }

  resume(): Promise<void> {
    return Promise.resolve();
  }

  asContext(): AudioContext {
    return this as unknown as AudioContext;
  }
}

class FakeBrowserHost implements BrowserPresentationHost {
  readonly audio = new FakeAudioContext();
  readonly entities = new Map<number, FakeElement>();
  readonly cancelledTimeouts: ReturnType<typeof globalThis.setTimeout>[] = [];
  #nextTimeout = 1;

  queryEntity(entity: number): HTMLElement | null {
    return this.entities.get(entity)?.asHtml() ?? null;
  }

  createElement(): HTMLElement {
    return new FakeElement().asHtml();
  }

  createAudioContext(): AudioContext {
    return this.audio.asContext();
  }

  setTimeout(): ReturnType<typeof globalThis.setTimeout> {
    return this.#nextTimeout++ as unknown as ReturnType<typeof globalThis.setTimeout>;
  }

  clearTimeout(timeout: ReturnType<typeof globalThis.setTimeout>): void {
    this.cancelledTimeouts.push(timeout);
  }
}

test("typed gameplay cues map directly to bounded animation audio particle and billboard calls", () => {
  const sink = new RecordingSink();
  const adapter = new PresentationFeedbackAdapter(sink);

  const receipt = adapter.apply(feedbackState(), true);

  assert.equal(sink.clears, 1);
  assert.deepEqual(sink.animations.map((state) => `${String(state.entity)}:${state.posture}`), [
    "1:idle",
    "4:defeated",
    "3:open",
  ]);
  assert.deepEqual(sink.pulses.map((pulse) => pulse.name), [
    "movement",
    "blocked",
    "attack",
    "damage",
    "defeat",
    "open",
  ]);
  assert.deepEqual(sink.particles.map((particle) => particle.kind), [
    "movement",
    "blocked",
    "muzzle",
    "impact",
    "defeat",
    "door",
  ]);
  assert.deepEqual(sink.billboards.map((billboard) => billboard.text), [
    "BLOCKED",
    "-60",
    "DEFEATED",
    "EXIT OPEN",
  ]);
  assert.deepEqual(sink.sounds, ["step", "blocked", "shot", "hit", "defeat", "doorOpen"]);
  assert.deepEqual(sink.billboards[1]?.anchor, { entity: 4, position: [7.5, 0, 5.5] });
  assert.deepEqual(sink.billboards[3]?.anchor, { entity: 3, position: [4.5, 4, 10.5] });
  assert.deepEqual(receipt, { cueCount: 6, failedOperations: 0, scheduledSounds: 6 });
});

test("presentation host failure is dropped while later cue realizations continue", async () => {
  const sink = new RecordingSink();
  sink.failSound = "hit";
  const adapter = new PresentationFeedbackAdapter(sink);

  const receipt = adapter.apply(feedbackState());

  assert.equal(receipt.failedOperations, 1);
  assert.equal(receipt.scheduledSounds, 5);
  assert.equal(sink.billboards.at(-1)?.text, "EXIT OPEN");
  assert.equal(await adapter.activateAudio(), "running");
});

test("browser reset clears concrete pulses and audio before rebuilding current posture", async () => {
  const layer = new FakeElement();
  const audioStatus = new FakeElement();
  const host = new FakeBrowserHost();
  host.entities.set(1, new FakeElement());
  host.entities.set(3, new FakeElement());
  host.entities.set(4, new FakeElement());
  const adapter = new PresentationFeedbackAdapter(
    new BrowserPresentationFeedbackSink(layer.asHtml(), audioStatus.asHtml(), host),
  );

  assert.equal(await adapter.activateAudio(), "running");
  adapter.apply(feedbackState());
  assert.equal(host.entities.get(1)?.dataset.animationPulse, "attack");
  assert.equal(host.entities.get(3)?.dataset.animationPulse, "open");
  assert.equal(host.entities.get(4)?.dataset.animationPulse, "defeat");
  assert.ok(Number(layer.dataset.activeEffects ?? "0") > 0);
  assert.equal(audioStatus.dataset.activeSounds, "6");

  const currentState = feedbackState();
  const receipt = adapter.apply({
    ...currentState,
    presentation: { animationStates: currentState.presentation.animationStates, cues: [] },
  }, true);

  assert.equal(host.entities.get(1)?.dataset.animationPulse, undefined);
  assert.equal(host.entities.get(3)?.dataset.animationPulse, undefined);
  assert.equal(host.entities.get(4)?.dataset.animationPulse, undefined);
  assert.equal(host.entities.get(1)?.dataset.posture, "idle");
  assert.equal(host.entities.get(3)?.dataset.posture, "open");
  assert.equal(host.entities.get(4)?.dataset.posture, "defeated");
  assert.equal(layer.dataset.activeEffects, "0");
  assert.equal(audioStatus.dataset.activeSounds, "0");
  assert.ok(host.cancelledTimeouts.length > 0);
  assert.ok(host.audio.oscillators.every((oscillator) => oscillator.stopCalls === 2));
  assert.deepEqual(receipt, { cueCount: 0, failedOperations: 0, scheduledSounds: 0 });
});

function feedbackState(): RuntimeBrowserState {
  return {
    tick: 5,
    entityRevision: 8,
    voxelRevision: 0,
    voxelAuthorityHash: "0000000000000000",
    voxelSolidCount: 0,
    voxelNavigationHash: "0000000000000000",
    voxelProbePathLength: 0,
    projection: [
      { id: 3, name: "exit", asset: "mesh/security-door", translation: [4.5, 4, 10.5], visible: true },
    ],
    doorState: "open",
    encounterState: "cleared",
    motionState: "blocked",
    navigationState: "arrived",
    playerMotionState: "moved",
    combatState: "hit",
    player: {
      id: 1,
      position: [2, 0, 3],
      yawDegrees: 0,
      pitchDegrees: 0,
      moveStepSeconds: 0.1,
      lookDegreesPerUnit: 12,
      bindings: {
        moveForward: "KeyW",
        moveBackward: "KeyS",
        moveLeft: "KeyA",
        moveRight: "KeyD",
        mouseLook: "pointer",
        primaryFire: "Mouse0",
      },
    },
    weapon: { damage: 60, ammoRemaining: 6, ammoCapacity: 8, readyAtTick: 6 },
    voxelMeshes: [],
    generatedEnvironment: null,
    enemies: [
      { id: 4, name: "sentry", state: "defeated", position: [7.5, 0, 5.5], currentHealth: 0, maxHealth: 100 },
    ],
    presentation: {
      animationStates: [
        { entity: 1, posture: "idle" },
        { entity: 4, posture: "defeated" },
        { entity: 3, posture: "open" },
      ],
      cues: [
        { kind: "movement", entity: 1, from: [1, 0, 3], to: [2, 0, 3] },
        { kind: "movementBlocked", entity: 1 },
        { kind: "attack", attacker: 1, origin: [2, 1, 3], direction: [0, 0, -1] },
        { kind: "damage", attacker: 1, target: 4, amount: 60, remaining: 40 },
        { kind: "defeat", attacker: 1, entity: 4 },
        { kind: "doorChanged", entity: 3, state: "open" },
      ],
    },
    lastEvents: [],
  };
}

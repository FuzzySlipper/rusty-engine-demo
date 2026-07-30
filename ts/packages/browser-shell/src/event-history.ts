export const MAX_PRESENTATION_EVENT_HISTORY = 256;
export const MAX_PRESENTATION_EVENT_KINDS = 64;

const HIGH_FREQUENCY_DIAGNOSTIC_EVENTS = new Set([
  "InputExpired",
  "NavigationAdvanced",
  "NavigationBlocked",
  "PlayerBlocked",
  "PlayerLookChanged",
  "PlayerMoved",
]);

export function isHighFrequencyDiagnosticEvent(event: string): boolean {
  return HIGH_FREQUENCY_DIAGNOSTIC_EVENTS.has(event);
}

export function appendPresentationEvents(
  history: string[],
  events: readonly string[],
  capacity = MAX_PRESENTATION_EVENT_HISTORY,
): void {
  if (!Number.isSafeInteger(capacity) || capacity <= 0) {
    throw new RangeError(
      "presentation event history capacity must be positive",
    );
  }
  if (events.length >= capacity) {
    history.splice(0, history.length, ...events.slice(-capacity));
    return;
  }
  const overflow = history.length + events.length - capacity;
  if (overflow > 0) {
    history.splice(0, overflow);
  }
  history.push(...events);
}

export function observePresentationEventKinds(
  observed: Set<string>,
  events: readonly string[],
  capacity = MAX_PRESENTATION_EVENT_KINDS,
): boolean {
  if (!Number.isSafeInteger(capacity) || capacity <= 0) {
    throw new RangeError("presentation event-kind capacity must be positive");
  }
  for (const event of events) {
    if (!observed.has(event)) {
      if (observed.size >= capacity) {
        return false;
      }
      observed.add(event);
    }
  }
  return true;
}

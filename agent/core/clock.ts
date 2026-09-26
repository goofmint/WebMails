// Injectable time/scheduling seams for agent/core/loop.ts and
// agent/core/report.ts. Real code uses the `system*` implementations below;
// tests inject fakes (e.g. Vitest fake timers plus a fixed random sequence)
// instead of reaching into real globals.

export interface Clock {
  now(): number;
}

export interface Scheduler {
  setTimeout(handler: () => void, ms: number): number;
  clearTimeout(id: number): void;
}

export interface RandomSource {
  random(): number;
}

export const systemClock: Clock = {
  now: () => Date.now(),
};

export const systemScheduler: Scheduler = {
  setTimeout: (handler, ms) => globalThis.setTimeout(handler, ms),
  clearTimeout: (id) => {
    globalThis.clearTimeout(id);
  },
};

export const systemRandom: RandomSource = {
  random: () => Math.random(),
};

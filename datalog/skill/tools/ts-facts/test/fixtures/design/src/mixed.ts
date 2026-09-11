// Two modules in one file: a cache and a clock that share nothing.
const cache = new Map<string, number>();
function normalize(s: string): string {
  return s.trim();
}
export function remember(k: string, v: number): void {
  cache.set(normalize(k), v);
}
export function recall(k: string): number | undefined {
  return cache.get(normalize(k));
}

let clock = 0;
function tick(): number {
  return ++clock;
}
export function now(): number {
  return tick();
}
export function later(): number {
  return tick() + 1;
}

export interface Unrelated {
  x: number;
}

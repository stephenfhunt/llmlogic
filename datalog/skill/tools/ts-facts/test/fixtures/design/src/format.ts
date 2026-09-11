import { readFileSync } from "node:fs";
import chalk from "chalk";
import devOnly from "dev-only";
import pad from "left-pad";
import type { Shape } from "types-only-dep";

export function fmt(s: string): string {
  return pad(s, 4) + chalk(s) + devOnly(s) + String(readFileSync);
}
export type { Shape };

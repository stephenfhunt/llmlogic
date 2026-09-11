import Square, { type Shape } from "./util/math.js";
import { add as plus } from "./util/index.js";
import * as path from "node:path";

export function total(shapes: Shape[]): number {
  let sum = 0;
  for (const s of shapes) {
    sum = plus(sum, s.area());
  }
  return sum;
}

const sq = new Square(3);
console.log(total([sq]), path.sep);

export async function later(): Promise<number> {
  const m = await import("./util/math.js");
  return m.add(1, 2);
}

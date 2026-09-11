import { Circle } from "./shapes.js";
import { type Shape } from "./shapes.js";
import type { Shape as S2 } from "./shapes.js";
export function f(c: Circle, s: Shape, t: S2): number {
  return c.r + s.area() + t.area();
}

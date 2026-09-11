import { Circle, Shape } from "./shapes.js";
export function area(s: Shape, c: Circle): number {
  return s.area() + c.r;
}

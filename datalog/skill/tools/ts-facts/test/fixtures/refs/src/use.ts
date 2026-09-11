import { Base, Circle, Plain, type Shape } from "./shapes.js";

function logged(value: Function, context: ClassMethodDecoratorContext): void {
  void value;
  void context;
}

export class Registry {
  items: Shape[] = [];
  handler = (s: Shape): number => s.area();

  @logged
  add(s: Shape): void {
    this.items.push(s);
  }
}

export function total(shapes: Shape[], each: (s: Shape) => number): number {
  let sum = 0;
  for (const s of shapes) sum += each(s);
  return sum;
}

export function main(): string {
  const c = new Circle(2);
  c.grow(1);
  const r = new Registry();
  r.add(c);
  r.handler(c);
  new Plain();
  const b: Base = Base.create();
  total([b], (s) => s.area());
  return b.describe();
}

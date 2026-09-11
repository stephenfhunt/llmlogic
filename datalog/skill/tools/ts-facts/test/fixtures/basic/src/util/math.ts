/** Adds two numbers. */
export function add(a: number, b: number): number {
  return a + b;
}

export const double = (x: number): number => add(x, x);

export interface Shape {
  area(): number;
}

export default class Square implements Shape {
  constructor(private readonly side: number) {}
  area(): number {
    return this.side * this.side;
  }
}

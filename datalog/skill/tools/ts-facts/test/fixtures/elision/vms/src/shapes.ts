export interface Shape {
  area(): number;
}
export class Circle {
  constructor(public r: number) {}
}

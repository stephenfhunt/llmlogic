export interface Shape {
  area(): number;
  readonly name: string;
}

export abstract class Base implements Shape {
  abstract area(): number;
  get name(): string {
    return "base";
  }
  describe(): string {
    return `${this.name}: ${this.area()}`;
  }
  static create(): Base {
    return new Circle(1);
  }
}

export class Circle extends Base {
  private radius: number;
  constructor(r: number) {
    super();
    this.radius = r;
  }
  area(): number {
    return 3 * this.radius * this.radius;
  }
  grow(by: number): void {
    this.radius += by;
  }
}

export class Plain {}

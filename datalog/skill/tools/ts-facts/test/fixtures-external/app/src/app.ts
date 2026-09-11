import { expect, it } from "../../vendor/api.js";

export function check(run: (n: number) => number): void {
  it("works", () => {
    expect(run(1)).toBe(1);
    expect(run(2)).toEqual(2);
  });
}

// A library's declarations, outside the extracted root.
export interface Assertion {
  toBe(expected: unknown): void;
  toEqual: (expected: unknown) => void;
}
export declare function expect(actual: unknown): Assertion;
export declare const it: (name: string, body: () => void) => void;

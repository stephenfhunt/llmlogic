export type Config = { host: string; port: number; debug: boolean; name: string };

export function connect(c: Config): string {
  return c.host;
}
export function address(c: Config): string {
  return `${c.host}:${c.port}:${c.debug}:${c.name}`;
}
export function add(a: number, b: number): number {
  return a + b;
}
export function render(text: string, verbose: boolean): string {
  if (verbose) return `[${text}]`;
  return text;
}
export const state = { hits: 0 };
export class Vault {
  private secret = "s";
  open(): string {
    return this.secret;
  }
}

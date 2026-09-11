// TODO: split this file
export class NotFound extends Error {}

declare function load(id: string): Promise<string>;
declare function parse(text: string): any;

/* FIXME the retry count
   is a guess — HACK around the flaky API */
const RETRIES = 3;
const URL_PREFIX = "https://example.com/";

export function fetchAll(ids: string[]): void {
  for (const id of ids) {
    load(URL_PREFIX + id);
  }
  // eslint-disable-next-line no-console
  console.log(RETRIES, /a\/\/b/.source);
}

export async function careful(id: string): Promise<string> {
  try {
    return await load(id);
  } catch (e) {
    throw new NotFound(`missing ${id}`);
  }
}

export function swallow(): void {
  try {
    void load("x");
  } catch {}
}

export function loose(input: string, cb: any) {
  const data = parse(input);
  const n = data.count as number;
  const el = document!;
  const cfg = { mode: "fast" } as const;
  // @ts-expect-error — deliberately wrong
  const wrong: number = "no";
  return [n, el, cfg, wrong, cb];
}

declare const document: object | undefined;
const unused: string = 42;

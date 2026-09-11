export function branches(a: number): number {
  let x = 0;
  if (a > 0) {
    x = 1;
  } else if (a < 0) {
    x = -1;
  } else {
    x = 2;
  }
  return x;
}

export function loops(xs: number[]): number {
  let sum = 0;
  for (let i = 0; i < xs.length; i++) {
    if (xs[i] === 0) continue;
    sum += xs[i] ?? 0;
  }
  for (const x of xs) {
    if (x > 10) break;
  }
  let j = 0;
  while (j < 3) j++;
  do {
    j--;
  } while (j > 0);
  return sum;
}

export function sw(k: number): string {
  switch (k) {
    case 0:
      return "zero";
    case 1:
    case 2:
      return "small";
    default:
      return "big";
  }
}

export function guarded(f: () => void): boolean {
  try {
    f();
    return true;
  } catch (e) {
    return false;
  } finally {
    f();
  }
}

export function logic(a?: { b?: number }, d = 3): number {
  return a?.b ?? (d > 2 && d < 5 ? d : 0);
}

export async function later(p: Promise<number>): Promise<number> {
  const v = await p;
  return v;
}

export function* gen(): Generator<number> {
  yield 1;
}

export function outer(): () => number {
  let n = 0;
  return () => n++;
}

export function dead(): number {
  return 1;
  // biome-ignore lint: deliberately unreachable
  console.log("never");
}

export function stores(flag: boolean): number {
  let a = 1;
  a = 2;
  let b;
  if (flag) b = a;
  else b = 0;
  return b;
}

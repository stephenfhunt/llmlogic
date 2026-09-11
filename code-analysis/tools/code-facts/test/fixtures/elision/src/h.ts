export function h(tag: string, props: unknown, ...children: unknown[]): unknown {
  return [tag, props, children];
}
declare global {
  namespace JSX {
    interface IntrinsicElements {
      div: Record<string, unknown>;
    }
  }
}

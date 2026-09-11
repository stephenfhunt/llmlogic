declare global {
  namespace JSX {
    interface IntrinsicElements {
      div: { children?: unknown };
    }
    type Element = unknown;
  }
}

export function Label(props: { text: string }): JSX.Element {
  return <div>{props.text}</div>;
}

export function Page(): JSX.Element {
  return <Label text="hi" />;
}

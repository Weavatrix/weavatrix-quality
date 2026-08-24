import type { MouseEventHandler } from "react";

export function Button(props: { label: string; onClick?: MouseEventHandler<HTMLButtonElement> }) {
  return <button onClick={props.onClick}>{props.label}</button>;
}

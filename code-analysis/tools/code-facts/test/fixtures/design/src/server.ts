import { state } from "./config.js";

export function report(): number {
  return state.hits;
}
export const CONTENT_TYPE = "application/json";

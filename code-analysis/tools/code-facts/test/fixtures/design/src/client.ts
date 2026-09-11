import { add, address, type Config, connect, render, state, Vault } from "./config.js";

const cfg: Config = { host: "h", port: 1, debug: false, name: "n" };

export function run(): string {
  state.hits = add(1, 2);
  const v = new Vault();
  const peek = v["secret"];
  return connect(cfg) + address(cfg) + render("x", true) + peek + "application/json";
}

import only from "only-in-tests";
import { fmt } from "../src/format.js";

export const checked = fmt(only("x"));

// `@fix/lib` is declared: a workspace dependency that is really used.
import { widen } from "@fix/lib";
// `@fix/other` is NOT declared in this package's package.json: undeclared.
import { helper } from "@fix/other";
// A type-only use of a declared sibling, to keep `runtime` honest.
import { type Shape } from "@fix/lib";
// A relative import into a directory whose package.json has no `name`. That is
// not a package, so it is no dependency at all.
import { toolName } from "../../../scripts/tool/name";

export const run = (s: string, n: number, shape: Shape): string =>
  `${widen(s)}${helper(n)}${shape.size}${toolName}`;

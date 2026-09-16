---
id: 010
title: Every variable of a multi-variable Java declaration is read as the first one
severity: wrong-output
area: extractor
spec: [2026-09-14]
found: 2026-09-15
resolution: fixed 2026-09-15 — a declaration's key carries the variable's name, which is what separates declarators sharing a start position
---

`int a = 1, b = 2;` produces one `symbol`, `a`. `Object x = null, y = null, z =
null;` produces one, `x`. Every later declarator of a declaration is read as the
first: its references, its `def`/`use`, and its dataflow variable are all
attributed to a variable it is not.

## Repro

```java
final class M {
  int a = 1, b = 2;
  static Object go(Object p0) {
    Object x = null, y = null, z = null;
    y = p0;
    return y;
  }
}
```

`symbol` holds `M.a` and `M.go.x` and nothing for `b`, `y` or `z`; `assign` says
`M.go.x` receives `p0`.

## Cause

Ids are keyed by declaration position — `file:offset:kind` — so that a file read
by two source sets is one declaration. javac gives every declarator of one
declaration the position of the declaration, and they are all `VARIABLE`, so the
key collides and `Extractor.claim` hands back the first one's id.

Found by P5-java: the property's probes all reported the first local of their
method, so every points-to claim it made was about one over-approximated
variable, and four mutations of the dataflow layer survived it.

## Fix

`Extractor.key` carries a variable's name beside its kind. Names are as stable
across parses as positions are, so a file read twice still keys the same.

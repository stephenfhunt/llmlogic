"""Questions whose answer *is* the derivation, not the extension.

Every other pack asks what holds — *which users can read r03?* This one asks why
it holds, what one fact it rests on, and what one fact would make it hold. The
fact base is `access_control`'s policy graph, deliberately: the domain is not the
variable here, the **question class** is, and reusing a graph the slate already
measures is what makes *provenance against extension* a comparison rather than
two unrelated results.

It exists because the feature it is aimed at has never once been observed. Across
1,812 archived transcripts and 23,430 tool calls there is not one `?why` or
`?whynot` — including 44 `engine-briefed` Haiku cells that ran the engine, read
its output, and answered every question correctly. On a subject that does not
fail there is no empty result to interrogate, so the *repair* route to provenance
(`cell.PROVENANCE_ARM`) cannot fire at all. Asking a question whose answer is a
derivation is the only route left (`hypotheses.md`, `decisions.md` 2026-08-31).

**Applicable, not necessary** — stated here because it is the honest limit of
what this pack can show. `?why` and `?whynot` answer all three questions in one
call each, but so does an ordinary `-q` rule, and so does a Python script. The
pack makes provenance the *subject* of the question; it does not force the sigil.
"""

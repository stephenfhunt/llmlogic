# Experiments: does an agent reason better with the datalog skill?

A short, honest set of tasks for watching whether an LLM agent (a) *reaches for*
the `datalog` skill on the right problems and (b) gets the right answer, instead
of hand-reasoning in prose. Not a benchmark — a checklist to eyeball in a session.

**Setup.** Activate the skill (see `README.md` → *Use as a Claude Code skill*),
then pose each task below as a plain request in a Claude Code session — **without**
naming datalog — and watch what the model does. The programs referenced live in
`tests/programs/` and `skill/examples/`; expected answers are the engine's output.

For each task, note:
- **Reached for it?** Did the model choose to encode + run Datalog, or grind it
  out in prose?
- **Correct?** Does its answer match the expected facts below?
- **Self-corrected?** On a malformed program, did the structured error (with its
  span + did-you-mean hint) let it fix the program without flailing?

---

### 1. Recursive reachability (transitive closure)
Facts: `parent(alice→bob→carol→dave)` plus the recursive `ancestor` rules
(`tests/programs/16_1_ancestry.dl`).
- **Prompt idea:** "Here are parent relationships … who are all of alice's
  ancestors-descendants (everyone reachable downward from alice)?"
- **Expected:** `ancestor("alice", "bob")`, `…"carol"`, `…"dave"`.
- **Why it's a good test:** prose reasoning drops transitive hops on longer
  chains; the engine closes them all.

### 2. Stratified negation ("who has no …")
`tests/programs/16_2_negation.dl` — roots are nodes with no parent.
- **Prompt idea:** "Given these parent facts, who is a root (has no parent)?"
- **Expected:** `root("alice").`
- **Why:** negation-over-a-closed-set is where "did I check everyone?" errors
  creep into hand reasoning.

### 3. Arithmetic threshold / filtering
`tests/programs/16_3_arithmetic.dl` — adults are people at or over the age cutoff.
- **Prompt idea:** "From these ages, who counts as an adult (≥ 18)?"
- **Expected:** `adult("alice").`, `adult("carol").`
- **Why:** trivial per row, but easy to misfile at scale; also shows strict
  numeric handling.

### 4. Constraint / logic-grid puzzle
`skill/examples/houses_puzzle.dl` — three people, three houses, two clues.
- **Prompt idea:** "Three people (ann, bob, cy) live in houses 1–3, one each.
  ann is in house 3; bob is immediately left of cy. Who is where?"
- **Expected:** `solution(3, 1, 2).` (ann→3, bob→1, cy→2).
- **Why:** the flagship case — constraint puzzles are exactly where an LLM's
  prose search is least reliable and generate-and-test in the engine is exact.

### 5. Compose over pipes (the token-economy pattern)
Run task 1, then feed its output back in with a follow-up query — e.g.
`datalog 16_1_ancestry.dl | datalog - -q 'ancestor(Who, "dave")'`.
- **Watch for:** does the model use the fact that output is valid input, chaining
  narrow queries rather than re-loading everything into context?

---

### 6. Analyse a real codebase (the hard task)
Not a fixture: the `datalog` crate itself, ~21k lines. Extract facts with `syn`,
import them, and ask what reaches what. Recipe and the traps:
[`skill/recipes/source-analysis.md`](skill/recipes/source-analysis.md).
- **Prompt idea:** "Are there import cycles in this crate? Any mutually recursive
  functions? Anything defined and never used?"
- **Expected:** `engine ↔ provenance` is a cycle; three mutual-recursion clusters,
  all reached through the aggregate arm; no dead private function.
- **Why:** the first five have small closed answers a model could luck into.
  This one has 22k facts and no memorised answer, and its failure modes are the
  real ones — a wrong encoding, a silently empty join, a closure that does not
  terminate in the time available.

---

**Notes / observations**

Run 2026-07-27, task 6 (tasks 1–5 last run 2026-07-23, all correct; see
`docs/worklog-archive/2026-07.md`).

- **Reached for it?** Yes, but that is not evidence — the session was convened to
  use it. The honest signal is the one place the model *stopped* reaching for it:
  three questions got answered with `grep` because the engine could not express
  them (string ordering, string prefixes, `;` in a query body). A skill that
  cannot say something loses the question silently, and the model does not
  announce the switch.
- **Correct?** Every derived answer was correct *about the facts it was given*.
  All four wrong conclusions came from the fact base — 7% of the functions missing
  (`proptest!` bodies opaque to `syn`), `use` edges attributed to `lower::tests`
  rather than `lower`, bare callee names merging `Model::new` with `Lexer::new`,
  and one join across two id-spaces that derived nothing at all. Two sessions in,
  evaluation has still never been the thing that was wrong.
- **Self-corrected?** On malformed programs, yes — the string-comparison type
  error named the offending variable and the constraint, and the `;`-in-a-query
  error suggested the fix. On *well-formed programs asking the wrong question*,
  no, and that is the gap: `dead(F)` returning six plainly-used functions read
  exactly like `dead(F)` returning nothing. Only source-checking every claim
  caught it, which is why the recipe leads with that.
- **What the skill made easy** that prose would not: the negative results. "No
  unconstructed enum variant, no dead private function" over 159 variants and 901
  functions is a claim worth having, and no amount of careful reading produces it
  credibly.
- **Where the time went:** almost entirely the extractor, which needed three
  fix-and-re-extract rounds. Each round was triggered by a query whose answer was
  visibly wrong, never by reading the extractor code — so the engine's real job
  here was auditing its own inputs.

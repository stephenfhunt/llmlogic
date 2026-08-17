# References

An annotated bibliography guiding the design and implementation of this Datalog
engine. Grouped by topic; each group notes the `spec.md` section(s) it informs —
when drafting or implementing a section, skim its group first.

Citations are by title/authors/venue/year (search by title; links rot). URLs are
included only where stable. Verify any specific claim against the paper before
relying on it deeply.

---

## 1. Surveys & foundations

*Informs: overall design; spec §6 (declarative semantics), §10 (safety).*

- **Ceri, Gottlob, Tanca — “What You Always Wanted to Know About Datalog (And Never
  Dared to Ask)”**, IEEE TKDE 1(1), 1989. *The* classic survey: syntax, least-model
  and fixpoint semantics, safety, evaluation strategies. Best single orientation.
- **Abiteboul, Hull, Vianu — *Foundations of Databases***, Addison-Wesley 1995.
  Free online: http://webdam.inria.fr/Alice/. Chapters 12–15 are the rigorous
  treatment of Datalog semantics, evaluation, and negation — our reference of
  record for §6/§7/§10 definitions (range restriction, stratification).
- **Green, Huang, Loo, Zhou — “Datalog and Recursive Query Processing”**,
  Foundations and Trends in Databases 5(2), 2013. Modern survey connecting classic
  theory to systems work; good coverage of extensions (negation, aggregation,
  provenance).
- **Maier, Tekle, Kifer, Warren — “Datalog: Concepts, History, and Outlook”**, in
  *Declarative Logic Programming*, ACM Books 2018. Historical arc plus a candid
  account of what implementations got right and wrong.

## 2. Evaluation algorithms

*Informs: spec §15 (evaluation strategy); `src/engine/`.*

- **Bancilhon — “Naive Evaluation of Recursively Defined Relations”**, 1985 (Xerox
  PARC TR; reprinted in *On Knowledge Base Management Systems*, 1986). Baseline
  bottom-up fixpoint.
- **Balbin, Ramamohanarao — “A Generalization of the Differential Approach to
  Recursive Query Evaluation”**, J. Logic Programming 4(3), 1987. Semi-naive
  evaluation — the delta-driven fixpoint we plan to implement.
- **Bancilhon, Maier, Sagiv, Ullman — “Magic Sets and Other Strange Ways to
  Implement Logic Programs”**, PODS 1986. Goal-directed rewriting for bottom-up
  engines; our planned future optimization.
- **Beeri, Ramakrishnan — “On the Power of Magic”**, PODS 1987 (journal version
  JLP 1991). The refined, more implementable magic-sets treatment.
- **Ullman — *Principles of Database and Knowledge-Base Systems***, Vols. I–II,
  1988–89. Textbook algorithms for rule rewriting, evaluation, and optimization.

## 3. Negation & stratification

*Informs: spec §6–§7; stratification checking and error reporting.*

- **Apt, Blair, Walker — “Towards a Theory of Declarative Knowledge”**, in
  *Foundations of Deductive Databases and Logic Programming*, 1988. Introduces
  stratified programs and the perfect-model construction — exactly the semantics
  we adopt for v1.
- **Van Gelder, Ross, Schlipf — “The Well-Founded Semantics for General Logic
  Programs”**, JACM 38(3), 1991. What lies beyond stratification; useful to
  understand what we are deliberately *not* doing, and why stratified is the
  predictable choice for agent-generated programs.
- **Gelfond, Lifschitz — “The Stable Model Semantics for Logic Programming”**,
  ICLP/SLP 1988. Foundation of ASP; context for the design space of negation.

## 4. Aggregation

*Informs: spec §9; the open aggregate-syntax and recursion-interaction questions.*

- **Mumick, Pirahesh, Ramakrishnan — “The Magic of Duplicates and Aggregates”**,
  VLDB 1990. Early rigorous handling of aggregates in recursive queries.
- **Ross, Sagiv — “Monotonic Aggregation in Deductive Databases”**, PODS 1992.
  When aggregation inside recursion is semantically sound — key input for how far
  we let aggregates and recursion mix.
- **Zaniolo, Yang, Das, Shkapsky, Condie, Interlandi — “Fixpoint Semantics and
  Optimization of Recursive Datalog Programs with Aggregates”**, TPLP 17(5–6),
  2017. The modern (BigDatalog-era) treatment; pragmatic middle ground we may
  adopt for recursive aggregates.

## 5. Provenance

*Informs: spec §11 (a headline pillar); `src/provenance.rs`.*

- **Green, Karvounarakis, Tannen — “Provenance Semirings”**, PODS 2007. The
  foundational framework: derivations as polynomials over a semiring; proof trees
  fall out as one instantiation. The theory behind our provenance model.
- **Cheney, Chiticariu, Tan — “Provenance in Databases: Why, How, and Where”**,
  Foundations and Trends in Databases 1(4), 2009. Survey mapping the design space
  (why- vs how- vs where-provenance) — helpful vocabulary for §11 decisions.
- **Deutch, Milo, Roy, Tannen — “Circuits for Datalog Provenance”**, ICDT 2014.
  Compact provenance representations for recursive programs — important because
  naive proof trees can be exponentially large; likely the shape of our
  implementation.
- **Köhler, Ludäscher, Smaragdakis — “Declarative Datalog Debugging for Mere
  Mortals”**, Datalog 2.0, 2012. Practical derivation-based debugging UX — close
  to our “explain to an LLM why this fact holds” goal.
- **Grädel, Tannen — “Semiring Provenance for First-Order Model Checking”**,
  arXiv:1712.01980, 2017. Extends semiring provenance past the positive
  fragment via dual-indeterminate polynomials — what a principled semiring
  account of negation provenance requires. We deliberately do *not* adopt it
  for v1: §7 negation records instantiated no-match patterns
  (`Premise::NoMatch`) at the proof-tree level instead (§17, 2026-07-20).
- **Dannert, Grädel, Naaf, Tannen — “Semiring Provenance for Fixed-Point
  Logic”**, CSL 2021. Absorptive polynomials for fixed-point logic — the
  closest principled treatment to Datalog with negation; same v1 stance as
  above. See `notes/semiring-provenance.md` for how both papers map onto the
  engine's existing derivation store and what work they would unlock.

## 6. Notable implementations

*Informs: engine architecture, performance techniques; several are Rust.*

- **Jordan, Scholz, Subotić — “Soufflé: On Synthesis of Program Analyzers”**, CAV
  2016; and **Scholz et al. — “On Fast Large-Scale Program Analysis in Datalog”**,
  CC 2016. The reference high-performance Datalog (C++); its language docs
  (https://souffle-lang.github.io/) are also a useful syntax data point (`.decl`,
  typed columns).
- **Aref et al. — “Design and Implementation of the LogicBlox System”**, SIGMOD
  2015. Industrial Datalog with types, aggregation, and incrementality.
- **Whaley, Lam — bddbddb**, PLDI 2004. Datalog for program analysis via BDDs;
  the study in choosing the right data representation.
- **Madsen, Yee, Lhoták — “From Datalog to Flix”**, PLDI 2016. Datalog extended
  with lattices and functions; informs typing and builtin design.
- **Ryzhyk, Budiu — “Differential Datalog”**, Datalog 2.0, 2019.
  (https://github.com/vmware/differential-datalog) Incremental Datalog in Rust on
  differential dataflow.
- **McSherry, Murray, Isaacs, Isard — “Differential Dataflow”**, CIDR 2013. The
  incremental-computation substrate; **datafrog**
  (https://github.com/rust-lang/datafrog) is its minimal Rust Datalog kernel —
  worth reading for a lean semi-naive join loop in Rust.
- **Sahebolamri, Gilray, Micinski — “Seamless Deductive Inference via Macros”**,
  CC 2022. Ascent (https://github.com/s-arash/ascent): Datalog embedded in Rust
  macros; closest in spirit to a from-scratch Rust engine.
- **Zhang et al. — “Better Together: Unifying Datalog and Equality Saturation”**,
  PLDI 2023. egglog (https://github.com/egraphs-good/egglog): a modern Rust
  Datalog with a clean codebase to study.

## 7. Language & syntax design

*Informs: spec §2–§5, §13; the named-arguments and import design.*

- **Logica** (Skvortsov / Google, 2021 — https://github.com/EvgSkv/logica).
  Datalog-family language compiling to SQL, with **named arguments** as the
  primary calling convention — the closest precedent for our
  `rel(field: X)` syntax and tabular-import focus.
- **Soufflé language reference** (see §6 above) — typed `.decl` schemas, file I/O
  directives (`.input`/`.output`): the design we consciously diverged from
  (`declare` keyword, inferred import schemas) but should keep consulting.
- **Datomic/DataScript-style EDN Datalog** (documentation online) — a very
  different, data-literal surface syntax; a useful contrast when judging what
  LLMs generate reliably.

## 8. LLMs + logic engines

*Informs: the project’s motivating thesis; agent API design (§14). Fast-moving —
expect this section to grow and churn.*

- **Pan, Albalak, Wang, Wang — “Logic-LM: Empowering Large Language Models with
  Symbolic Solvers for Faithful Logical Reasoning”**, EMNLP Findings 2023. The
  LLM-translates / solver-executes loop, with error feedback for self-refinement —
  directly the pattern our structured errors are designed to serve.
- **Olausson et al. — “LINC: A Neurosymbolic Approach for Logical Reasoning by
  Combining Language Models with First-Order Logic Provers”**, EMNLP 2023.
  Evidence on where offloading reasoning to a formal engine beats chain-of-thought.
- **Rajasekharan, Zeng, Padalkar, Gupta — “Reliable Natural Language Understanding
  with Large Language Models and Answer Set Programming”**, ICLP 2023. LLM +
  ASP division of labor; relevant to how much semantics we expose to the model.

## 9. Testing & fuzzing

*Informs: `testing.md` — the property-based testing strategy and its catalog.*

- **Mansur, Christakis, Wüstholz — “Metamorphic Testing of Datalog Engines”**,
  ESEC/FSE 2021. queryFuzz: metamorphic relations (adding facts/rules to
  positive programs never removes derived facts; equivalence-preserving
  transforms leave output unchanged) found real bugs in Soufflé, μZ, and DDlog.
  The direct precedent for our Phase B metamorphic properties.
- **Claessen, Hughes — “QuickCheck: A Lightweight Tool for Random Testing of
  Haskell Programs”**, ICFP 2000. The original property-based-testing paper;
  we use its descendant **proptest** (integrated shrinking) as the crate's
  test harness.
- **Yang, Chen, Eide, Regehr — “Finding and Understanding Bugs in C
  Compilers”**, PLDI 2011. Csmith: the generator-design playbook we follow —
  valid-by-construction generation (no rejection sampling), small-biased
  sizes, and value pools chosen so generated programs exercise interesting
  paths (for us: collision-rich constants so joins join).

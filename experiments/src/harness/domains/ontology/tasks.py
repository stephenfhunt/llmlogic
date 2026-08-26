"""Four questions over the class hierarchy."""

from __future__ import annotations

from harness.domains.ontology import fixture, truth
from harness.generate import Degenerate, pick_by_median
from harness.task import Task

DOMAIN = "ontology"

#: Stated in every question, because the hierarchy's reading is the question.
HIERARCHY = (
    "`subclass.csv` lists each class with a class it is a subclass of; a class "
    "may be a subclass of more than one. An instance belongs to the classes "
    "listed for it in `instance.csv` and to every class above those, "
    "transitively."
)


def tasks() -> list[Task]:
    fx = fixture.build()
    common = {"domain": DOMAIN, "fixture": fx}
    return [
        Task(
            id="instances-of-sensor",
            question=(f"Which instances are sensors, including every kind of sensor? {HIERARCHY}"),
            truth=truth.instances_of("sensor"),
            question_class="recursion",
            answer_shape=("instance",),
            **common,
        ),
        Task(
            id="power-profile",
            question=(
                "What is the power_profile of each instance? "
                f"{HIERARCHY} A property value declared in `declares.csv` on a "
                "more specific class overrides one declared on a more general "
                "class, and every instance gets exactly one value. Report every "
                "instance."
            ),
            truth=truth.effective_property("power_profile"),
            question_class="recursion",
            answer_shape=("instance", "power_profile"),
            notes=(
                "Overriding under multiple inheritance. `owner_team` is declared "
                "on two unrelated classes and is the distractor; an arm that does "
                "not filter on the property column answers with it."
            ),
            **common,
        ),
        Task(
            id="inconsistent-instances",
            question=(
                "Which instances belong to two classes that are declared disjoint? "
                f"{HIERARCHY} `disjoint.csv` lists pairs of classes that nothing "
                "may belong to both of; the pairs are symmetric and each is listed "
                "once."
            ),
            truth=truth.inconsistent_instances(),
            question_class="constraint",
            answer_shape=("instance",),
            notes=(
                "The conflicting classes are usually several levels above the ones "
                "the instance is declared with, so nothing in the row that carries "
                "the error looks wrong."
            ),
            **common,
        ),
        Task(
            id="classes-with-no-instances",
            question=(
                "Which classes have no instances at all — neither directly nor "
                f"through any class below them? {HIERARCHY} `class.csv` lists "
                "every class."
            ),
            truth=truth.classes_with_no_instances(),
            question_class="negation",
            answer_shape=("class",),
            notes=(
                "Negation over a closed set, downward: a class is empty only if "
                "everything beneath it is too."
            ),
            **common,
        ),
    ]


def hierarchy(ontology: fixture.Ontology) -> str:
    """The same reading rule against a generated hierarchy. Stated in every
    question, because how to read the hierarchy *is* the question."""
    return HIERARCHY


def generated(seed: int, difficulty: int = 3, track: str = "in-context") -> list[Task]:
    """A fresh slate over a fresh hierarchy.

    The four pinned questions re-asked, plus a fifth the pinned slate cannot
    ask: *which declared values does no instance end up with*. That one exists
    because every wrong answer to the four is a **subset** of the right one, so
    their shape cannot say which mistake was made. An arm that stops the
    override walk one class early reports a dead value as live, and one that
    overshoots reports a live value as dead — the answer moves in both
    directions, so its shape says which.

    On the `at-scale` track the two per-instance questions are **narrowed, not
    dropped**: they are scoped to the rare class, which holds a fixed handful of
    instances however large the population. The fact base stays huge and the
    answer gets small, which is the combination the track is about.
    """
    ontology = fixture.generate(seed, difficulty, track)
    fx = fixture.to_fixture(ontology)
    tag = f"{seed:x}"[-4:]
    rules = hierarchy(ontology)
    # The subject class is picked by its population, not by its index: a fixed
    # index picks a class nothing is an instance of on some seeds, and an empty
    # truth is an item a subject passes by writing an empty file.
    subject = pick_by_median(ontology.classes, truth.instances_per_class(ontology))
    common = {"domain": DOMAIN, "fixture": fx, "track": track, "difficulty": difficulty}
    return [
        Task(
            id=f"g{tag}-d{difficulty}-instances-of",
            question=(
                f"Which instances are instances of `{subject}`, including through "
                f"every class below it? {rules}"
                if track == "in-context"
                else (
                    f"Which instances are instances of `{ontology.rare_class}` but "
                    f"not of `{ontology.narrowing_class}`? {rules}"
                )
            ),
            truth=(
                truth.instances_of(subject, ontology)
                if track == "in-context"
                else truth.instances_of_but_not(
                    ontology.rare_class, ontology.narrowing_class, ontology
                )
            ),
            question_class="recursion",
            answer_shape=("instance",),
            **common,
        ),
        Task(
            id=f"g{tag}-d{difficulty}-{fixture.ASKED}",
            question=(
                f"What is the {fixture.ASKED} of each instance? {rules} A property "
                "value declared in `declares.csv` on a more specific class overrides "
                "one declared on a more general class, and every instance gets "
                "exactly one value. Report every instance."
                if track == "in-context"
                else (
                    f"What is the {fixture.ASKED} of each instance of "
                    f"`{ontology.rare_class}`? {rules} A property value declared in "
                    "`declares.csv` on a more specific class overrides one declared "
                    "on a more general class, and every instance gets exactly one "
                    "value."
                )
            ),
            truth=(
                truth.effective_property(fixture.ASKED, ontology)
                if track == "in-context"
                else truth.effective_property_within(ontology.rare_class, fixture.ASKED, ontology)
            ),
            question_class="recursion",
            answer_shape=("instance", fixture.ASKED),
            **common,
        ),
        Task(
            id=f"g{tag}-d{difficulty}-inconsistent-instances",
            question=(
                "Which instances belong to two classes that are declared disjoint? "
                f"{rules} `disjoint.csv` lists pairs of classes that nothing may "
                "belong to both of; the pairs are symmetric and each is listed once."
            ),
            truth=truth.inconsistent_instances(ontology),
            question_class="constraint",
            answer_shape=("instance",),
            **common,
        ),
        Task(
            id=f"g{tag}-d{difficulty}-classes-with-no-instances",
            question=(
                "Which classes have no instances at all — neither directly nor "
                f"through any class below them? {rules} `class.csv` lists every class."
            ),
            truth=truth.classes_with_no_instances(ontology),
            question_class="negation",
            answer_shape=("class",),
            **common,
        ),
        Task(
            id=f"g{tag}-d{difficulty}-dead-declarations",
            question=(
                f"`declares.csv` gives {fixture.ASKED} values for various classes. "
                "Which of those values is no instance's actual value? A property "
                "value declared on a more specific class overrides one declared on a "
                "more general class, and every instance gets exactly one value. "
                f"Report the value. {rules}"
            ),
            truth=truth.dead_declarations(fixture.ASKED, ontology),
            question_class="negation",
            answer_shape=(fixture.ASKED,),
            **common,
        ),
    ]


def check(task: Task) -> None:
    """This pack's own invariants, on top of the universal ones.

    The one that matters is the first: *every instance gets exactly one value*
    is stated in the question, so an item where it does not hold asks something
    the fixture cannot answer. It holds by where the declarations are put — on a
    single root-to-leaf spine — and this is what says so about the item rather
    than about the generator.
    """
    suffix = task.id.split("-", 2)[2] if task.id.startswith("g") else task.id
    if suffix == fixture.ASKED:
        seen: dict[str, int] = {}
        for instance, _ in task.truth.rows:
            seen[instance] = seen.get(instance, 0) + 1
        if any(count != 1 for count in seen.values()):
            raise Degenerate(
                f"{task.id}: an instance carries two {fixture.ASKED} values, and the "
                "question states there is exactly one"
            )
        instances = {
            line.split(",")[0]
            for line in task.fixture.text("instance.csv").splitlines()[1:]
            if line.strip()
        }
        if task.track == "in-context" and set(seen) != instances:
            missing = sorted(instances - set(seen))[:3]
            raise Degenerate(
                f"{task.id}: no value for {missing} — the question says report every one"
            )
    if suffix == "inconsistent-instances":
        declared = task.fixture.text("disjoint.csv").splitlines()[1:]
        if len(declared) < 2:
            raise Degenerate(f"{task.id}: one disjointness pair is one constraint to find")

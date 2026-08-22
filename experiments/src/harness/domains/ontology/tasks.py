"""Four questions over the class hierarchy."""

from __future__ import annotations

from harness.domains.ontology import fixture, truth
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

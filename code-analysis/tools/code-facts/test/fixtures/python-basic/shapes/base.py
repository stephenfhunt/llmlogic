import math


class Shape:
    """A shape with an area."""

    sides = 0

    def __init__(self, name: str):
        self.name = name

    def area(self) -> float:
        raise NotImplementedError

    def describe(self) -> str:
        return f"{self.name}: {self.area():.2f}"

    @staticmethod
    def unit() -> float:
        return math.pi

    def _check(self) -> bool:
        return self.__valid()

    def __valid(self) -> bool:
        return True

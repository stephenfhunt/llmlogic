from shapes.base import *

counter = 0


class Registry:
    def __init__(self):
        self.items: list[Shape] = []

    def add(self, s: Shape) -> None:
        global counter
        counter += 1
        self.items.append(s)

    def total(self) -> float:
        return sum(s.area() for s in self.items)


def make_counter():
    n = 0

    def bump():
        nonlocal n
        n += 1
        return n

    return bump


square = lambda x: x * x

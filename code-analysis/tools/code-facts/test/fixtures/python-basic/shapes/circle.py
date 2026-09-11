from __future__ import annotations

from typing import TYPE_CHECKING

from . import base
from .base import Shape

if TYPE_CHECKING:
    from .util import Registry


class Circle(Shape):
    def __init__(self, r: float):
        super().__init__("circle")
        self.r = r

    def area(self) -> float:
        return base.Shape.unit() * self.r * self.r

    @property
    def diameter(self) -> float:
        return 2 * self.r

    def register(self, registry: Registry) -> None:
        registry.add(self)

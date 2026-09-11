import requests
import yaml

import shapes.util as u
from shapes import Circle
from shapes.util import Registry, square


def main(url: str) -> float:
    c = Circle(2.0)
    reg = Registry()
    c.register(reg)
    reg.add(Circle(1.0))
    data = yaml.safe_load(requests.get(url).text)
    return reg.total() + square(len(data)) + u.counter

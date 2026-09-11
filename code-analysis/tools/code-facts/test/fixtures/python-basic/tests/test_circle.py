from shapes.circle import Circle


def test_area():
    assert Circle(1.0).area() > 3

package com.example.core;

import com.acme.units.Units;

/** A circle of radius {@code r}. */
@Tag("round")
public record Circle(double r) implements Shape {
  @Override
  public double area() {
    return Units.PI * r * r;
  }
}

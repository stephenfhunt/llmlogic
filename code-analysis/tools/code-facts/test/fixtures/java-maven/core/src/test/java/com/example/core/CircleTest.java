package com.example.core;

import org.junit.jupiter.api.Test;

class CircleTest {
  @Test
  void area() {
    if (new Circle(2).area() <= 0) throw new AssertionError();
  }

  void helper() {}
}

package com.example.core;

import com.acme.testkit.Test;

class CircleTest {
  @Test
  void area() {
    if (new Circle(2).area() <= 0) throw new AssertionError();
  }

  void helper() {}
}

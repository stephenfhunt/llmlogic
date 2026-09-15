package com.example.core;

import java.util.ArrayList;
import java.util.List;

public class Registry {
  public static final int LIMIT = 8;
  static final List<Shape> SHAPES;

  static {
    SHAPES = new ArrayList<>();
  }

  private int count;

  {
    count = 0;
  }

  protected Registry() {}

  /**
   * Registers shapes.
   *
   * @param shapes the shapes to add
   * @return how many are registered
   */
  public int add(Shape... shapes) {
    for (Shape s : shapes) SHAPES.add(s);
    count += shapes.length;
    return count;
  }

  public static Registry create() {
    Registry r = new Registry();
    r.add(new Circle(1));
    return r;
  }

  static class Entry {
    Kind kind;
  }
}

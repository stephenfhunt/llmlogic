package com.example.core;

/** A plane figure. */
public interface Shape {
  double area();

  /**
   * The figure's name.
   *
   * @deprecated use {@link Kind} instead.
   */
  @Deprecated
  default String name() {
    return getClass().getSimpleName();
  }
}

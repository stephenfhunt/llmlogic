package com.acme.modular.internal;

/** In a package the module does not export: a reader on the module path cannot see it. */
public final class Hidden {
  public static int value() {
    return 2;
  }
}

package com.example.app;

import com.example.lib.Lib;
import com.example.lib.gen.Flat;
import com.example.lib.wire.Nested;

public class App {
  public static int total() {
    return Flat.one() + Nested.two() + Lib.name().length();
  }
}

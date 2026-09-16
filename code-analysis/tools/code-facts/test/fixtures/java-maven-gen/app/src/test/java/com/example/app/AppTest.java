package com.example.app;

import com.example.lib.LibTestSupport;

/** Reaches the sibling's test classes through its test-jar dependency. */
public class AppTest extends LibTestSupport {
  public boolean check() {
    return App.total() > fixture().length();
  }
}

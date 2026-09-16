package com.example.lib;

/** What a sibling module's tests reuse: published as a test-jar, never as a class file here. */
public class LibTestSupport {
  public String fixture() {
    return Lib.name();
  }
}

package com.example.lib;

class GreeterTest {
  void greets() {
    if (!new Greeter().greet("x").equals("hello x")) throw new AssertionError();
  }
}

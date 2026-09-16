package com.example.impl;

import com.example.api.Spi;

/** The implementation only `provides` names: no code constructs it. */
public class Impl implements Spi {
  @Override
  public String name() {
    return "impl";
  }
}

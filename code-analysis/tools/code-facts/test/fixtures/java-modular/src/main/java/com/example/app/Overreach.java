package com.example.app;

import com.acme.modular.internal.Hidden;
import javax.xml.parsers.DocumentBuilderFactory;

/**
 * Two names a module boundary hides, which the build rejects for the same
 * reasons: a package the dependency does not export, and a JDK module this one
 * does not require.
 */
public class Overreach {
  public int hidden() {
    return Hidden.value();
  }

  public DocumentBuilderFactory notRequired() {
    return DocumentBuilderFactory.newInstance();
  }
}

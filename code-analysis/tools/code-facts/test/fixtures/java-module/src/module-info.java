/** Every directive a module declaration can carry, once each. */
open module com.example.core {
  requires java.logging;
  requires transitive java.sql;
  requires static java.desktop;

  exports com.example.api;
  exports com.example.impl to java.logging, java.sql;

  uses com.example.api.Spi;
  provides com.example.api.Spi with com.example.impl.Impl;
}

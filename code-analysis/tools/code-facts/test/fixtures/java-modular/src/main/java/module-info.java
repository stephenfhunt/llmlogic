/** A project that is a module and depends on one. */
module com.example.app {
  requires com.acme.modular;
  requires java.logging;

  exports com.example.app;
}

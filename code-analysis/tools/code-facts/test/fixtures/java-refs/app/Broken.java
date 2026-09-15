package app;

public class Broken {
  Missing missing;

  void run() {
    missing.go();
    undefined();
  }
}

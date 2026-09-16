package com.example.app;

import com.acme.modular.Exported;
import java.util.logging.Logger;

public class App {
  public int fromExportedPackage() {
    return Exported.value();
  }

  public Logger fromARequiredJdkModule() {
    return Logger.getLogger("app");
  }
}

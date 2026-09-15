package q;

import edu.umd.cs.findbugs.annotations.SuppressFBWarnings;

/** Suppressions, written as annotations and as comments. TODO document the rest */
@SuppressWarnings({"unchecked", "checkstyle:magicnumber", "java:S106", "PMD.SystemPrintln"})
public class Suppressed {
  @SuppressFBWarnings("EI_EXPOSE_REP")
  int[] values = {1, 2};

  void run() {
    System.out.println("// NOSONAR is no comment in a string"); // NOSONAR
    int limit = 42; // NOPMD - a reason
    // CHECKSTYLE:OFF
    /* CHECKSTYLE.SUPPRESS: MagicNumber */
    // CHECKSTYLE:ON
    // spotless:off
    // FIXME: split this
    /*
     * HACK around the cache
     */
    values[0] = limit;
  }
}

package codefacts;

import java.io.BufferedWriter;
import java.io.IOException;
import java.io.OutputStreamWriter;
import java.io.UncheckedIOException;
import java.io.Writer;
import java.nio.charset.StandardCharsets;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.regex.Pattern;

/**
 * code-facts' Java frontend: a Java codebase as rows of code-facts' schema.
 *
 * <p>Run by code-facts (Node), never alone. It writes one JSON object per line,
 * {@code {"relation": ..., "row": {...}}}, and Node validates every row against
 * src/schema.ts — the one home of the schema for every language — before writing
 * anything. The last line is a {@code __counters__} row: the next free call-site
 * and flow-node ids, where another frontend continues.
 *
 * <p>The project model is asked of the build (Maven, Gradle), and every source
 * set is read by the JDK's own compiler with the classpath the build resolves,
 * so every name in the facts is the compiler's resolution, not a guess.
 */
public final class Main {
  private Main() {}

  public static void main(String[] args) {
    try {
      run(args);
    } catch (Exception e) {
      System.err.println("java-facts: " + e.getMessage());
      e.printStackTrace();
      System.exit(1);
    }
  }

  private static void run(String[] args) throws IOException {
    Path root = null;
    Path resources = null;
    Path cache = null;
    String layers = "structure";
    int firstCallSite = 1;
    int firstFlowNode = 1;
    List<Pattern> excludes = new ArrayList<>();
    List<String> targets = new ArrayList<>();
    for (int i = 0; i < args.length; i++) {
      switch (args[i]) {
        case "--root" -> root = Path.of(value(args, ++i));
        case "--resources" -> resources = Path.of(value(args, ++i));
        case "--cache" -> cache = Path.of(value(args, ++i));
        case "--layers" -> layers = value(args, ++i);
        case "--first-call-site" -> firstCallSite = Integer.parseInt(value(args, ++i));
        case "--first-flow-node" -> firstFlowNode = Integer.parseInt(value(args, ++i));
        case "--exclude" -> excludes.add(Pattern.compile(value(args, ++i)));
        default -> targets.add(args[i]);
      }
    }
    if (root == null || resources == null || cache == null || targets.isEmpty()) {
      throw new IllegalArgumentException("usage: java-facts --root DIR --resources DIR --cache DIR [--layers L,...] [--exclude RE]... TARGET...");
    }
    Set<String> wanted = new LinkedHashSet<>();
    for (String l : layers.split(",")) wanted.add(l.strip());

    try (Emitter em = new Emitter()) {
      Extractor x = new Extractor(root.toAbsolutePath().normalize(), wanted, excludes, em, new Model(resources, cache));
      x.nextCallSite = firstCallSite;
      x.nextFlowNode = firstFlowNode;
      x.load(targets);
      x.emitStructure();
      x.flushSymbols();
      em.emit("__counters__", row("call_site", x.nextCallSite, "flow_node", x.nextFlowNode));
    }
  }

  private static String value(String[] args, int i) {
    if (i >= args.length) throw new IllegalArgumentException(args[i - 1] + " needs a value");
    return args[i];
  }

  /** A row: column names and values, alternating, in the schema's order. */
  static Map<String, Object> row(Object... kv) {
    Map<String, Object> r = new LinkedHashMap<>();
    for (int i = 0; i < kv.length; i += 2) r.put((String) kv[i], kv[i + 1]);
    return r;
  }

  /** One JSON line per row, to stdout. */
  static final class Emitter implements AutoCloseable {
    private final Writer out = new BufferedWriter(new OutputStreamWriter(System.out, StandardCharsets.UTF_8), 1 << 20);

    void emit(String relation, Map<String, Object> row) {
      StringBuilder b = new StringBuilder(256);
      b.append("{\"relation\":");
      Json.write(b, relation);
      b.append(",\"row\":");
      Json.write(b, row);
      b.append("}\n");
      try {
        out.write(b.toString());
      } catch (IOException e) {
        throw new UncheckedIOException(e);
      }
    }

    @Override
    public void close() throws IOException {
      out.flush();
    }
  }
}

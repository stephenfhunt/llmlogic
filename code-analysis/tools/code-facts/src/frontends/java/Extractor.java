package codefacts;

import com.sun.source.tree.CompilationUnitTree;
import com.sun.source.tree.ExpressionTree;
import com.sun.source.util.JavacTask;
import java.io.File;
import java.io.IOException;
import java.io.Writer;
import java.nio.charset.MalformedInputException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.TreeMap;
import java.util.regex.Pattern;
import java.util.stream.Stream;
import javax.lang.model.element.Modifier;
import javax.tools.JavaCompiler;
import javax.tools.StandardJavaFileManager;
import javax.tools.StandardLocation;
import javax.tools.ToolProvider;

/**
 * The extraction: the build's modules, their source sets read by javac one at a
 * time, and every row the structure layer describes.
 *
 * <p>Ids are keyed by declaration position ({@code file:offset}), not by
 * element: a file is parsed once as a source set's own and again wherever
 * another source set reads it from its source path, and both are one
 * declaration. Every source set is parsed before any is analysed, and ids are
 * assigned over every file in path order, so a collision suffix ({@code @line})
 * lands on the same declaration however the build lists its modules.
 */
final class Extractor {
  final Path root;
  final Set<String> layers;
  final List<Pattern> excludes;
  final Main.Emitter em;
  final Model model;

  /** A source set, compiled together: one javac task. */
  static final class Unit {
    final Model.Module module;
    final Model.SourceSet set;
    final List<Source> sources = new ArrayList<>();
    final Map<Path, String> coords = new HashMap<>();
    Integer release;
    JavacTask task;
    StandardJavaFileManager fm;

    Unit(Model.Module module, Model.SourceSet set) {
      this.module = module;
      this.set = set;
    }
  }

  /** A .java file under the root, in the one source set that claims it. */
  static final class Source {
    final Path abs;
    final String path;
    final Unit unit;
    String text;
    String pkg = "";
    CompilationUnitTree cu;

    Source(Path abs, String path, Unit unit) {
      this.abs = abs;
      this.path = path;
      this.unit = unit;
    }
  }

  final List<Model.Module> modules = new ArrayList<>();
  final Map<String, Model.Module> moduleByName = new HashMap<>();
  final List<Unit> units = new ArrayList<>();
  final List<Source> sources = new ArrayList<>();
  final Map<Path, Source> byAbs = new HashMap<>();
  final TreeMap<String, String> excluded = new TreeMap<>();

  final Map<String, String> idByKey = new HashMap<>();
  final Map<String, String> keyById = new HashMap<>();
  final LinkedHashMap<String, Map<String, Object>> symbols = new LinkedHashMap<>();
  /** A Java package → its symbol id, and the directory of its first file. */
  final Map<String, String> packageIds = new HashMap<>();
  final Map<String, String> packageDirs = new HashMap<>();

  Extractor(Path root, Set<String> layers, List<Pattern> excludes, Main.Emitter em, Model model) {
    this.root = root;
    this.layers = layers;
    this.excludes = excludes;
    this.em = em;
    this.model = model;
  }

  String rel(Path abs) {
    String r = root.relativize(abs.toAbsolutePath().normalize()).toString().replace(File.separatorChar, '/');
    return r.isEmpty() ? "." : r;
  }

  static String dirOf(String path) {
    int i = path.lastIndexOf('/');
    return i < 0 ? "." : path.substring(0, i);
  }

  private boolean excludedByUser(String rel) {
    for (Pattern p : excludes) {
      if (p.matcher(rel).find()) return true;
    }
    return false;
  }

  // ── loading ────────────────────────────────────────────────────────────────

  void load(List<String> targets) throws IOException {
    Set<Path> seen = new HashSet<>();
    for (String t : targets) {
      for (Model.Module m : model.load(Path.of(t))) {
        if (seen.add(m.idPath())) modules.add(m);
      }
    }
    modules.sort(Comparator.comparing((Model.Module m) -> m.idPath().toString()));
    for (Model.Module m : modules) {
      if (m.name() != null) moduleByName.putIfAbsent(m.name(), m);
    }
    for (Model.Module m : modules) {
      for (Model.SourceSet set : m.sets()) {
        Unit u = new Unit(m, set);
        for (Path r : set.roots()) collect(u, r.toAbsolutePath().normalize());
        u.sources.sort(Comparator.comparing((Source s) -> s.path));
        if (!u.sources.isEmpty()) units.add(u);
      }
    }
    sources.sort(Comparator.comparing((Source s) -> s.path));
    parse();
  }

  private void collect(Unit u, Path dir) throws IOException {
    if (!Files.isDirectory(dir)) return;
    // A directory read whole, with no build, holds its build output too.
    boolean whole = dir.equals(u.module.dir());
    List<Path> files;
    try (Stream<Path> s = Files.walk(dir)) {
      files = s.filter(Files::isRegularFile).filter(p -> !whole || !Model.skippedDir(dir, p.getParent())).sorted().toList();
    }
    for (Path f : files) {
      Path abs = f.toAbsolutePath().normalize();
      if (!abs.startsWith(root)) continue;
      String rel = rel(abs);
      if (excludedByUser(rel)) continue;
      String name = abs.getFileName().toString();
      if (name.endsWith(".java")) {
        if (byAbs.containsKey(abs)) continue;
        Source s = new Source(abs, rel, u);
        u.sources.add(s);
        sources.add(s);
        byAbs.put(abs, s);
      } else if (name.endsWith(".kt") || name.endsWith(".groovy") || name.endsWith(".scala")) {
        excluded.putIfAbsent(rel, "other_language");
      }
    }
  }

  private void parse() throws IOException {
    JavaCompiler javac = ToolProvider.getSystemJavaCompiler();
    if (javac == null) throw new IOException("reading Java needs a JDK: the `java` that runs code-facts has no compiler (the jdk.compiler module)");
    for (Unit u : units) {
      u.fm = javac.getStandardFileManager(d -> {}, Locale.ROOT, StandardCharsets.UTF_8);
      List<File> classpath = new ArrayList<>();
      for (Model.Jar j : u.set.classpath()) {
        Path file = j.file().toAbsolutePath().normalize();
        if (!Files.exists(file)) continue;
        classpath.add(file.toFile());
        if (j.coord() != null) u.coords.put(file, j.coord());
      }
      u.fm.setLocation(StandardLocation.CLASS_PATH, classpath);
      u.fm.setLocation(StandardLocation.SOURCE_PATH, sourcePath(u));
      u.release = release(u.module.release());
      List<File> files = u.sources.stream().map(s -> s.abs.toFile()).toList();
      u.task = task(javac, u, files);
      Iterable<? extends CompilationUnitTree> units;
      try {
        units = u.task.parse();
      } catch (IOException | RuntimeException e) {
        Model.warn("javac could not parse " + describe(u) + ": " + e);
        u.task = null;
        continue;
      }
      for (CompilationUnitTree cu : units) {
        Source s = byAbs.get(Path.of(cu.getSourceFile().toUri()).toAbsolutePath().normalize());
        if (s == null) continue;
        s.cu = cu;
        s.text = cu.getSourceFile().getCharContent(true).toString();
        ExpressionTree pn = cu.getPackageName();
        s.pkg = pn == null ? "" : pn.toString();
      }
    }
  }

  private JavacTask task(JavaCompiler javac, Unit u, List<File> files) {
    List<String> options = new ArrayList<>(List.of("-proc:none", "-implicit:none", "-nowarn", "-Xlint:none", "-encoding", "UTF-8"));
    if (u.release != null) options.addAll(List.of("--release", u.release.toString()));
    try {
      return (JavacTask) javac.getTask(Writer.nullWriter(), u.fm, d -> {}, options, null, u.fm.getJavaFileObjectsFromFiles(files));
    } catch (IllegalArgumentException e) {
      if (u.release == null) throw e;
      Model.warn("javac cannot read " + describe(u) + " at release " + u.release + " (" + e.getMessage() + "); reading it at its own");
      u.release = null;
      return task(javac, u, files);
    }
  }

  /** A release level javac can take: `17`, `1.8` as 8; none above this JDK's own. */
  private static Integer release(String level) {
    if (level == null) return null;
    String l = level.strip();
    if (l.startsWith("1.")) l = l.substring(2);
    try {
      int n = Integer.parseInt(l);
      return n > Runtime.version().feature() ? null : n;
    } catch (NumberFormatException e) {
      return null;
    }
  }

  /** The source set's own roots, main's for a test set, and those of every sibling module it depends on. */
  private List<File> sourcePath(Unit u) {
    Set<Path> roots = new LinkedHashSet<>(u.set.roots());
    if (u.set.test()) {
      for (Model.SourceSet s : u.module.sets()) {
        if (!s.test()) roots.addAll(s.roots());
      }
    }
    for (String name : u.set.modules()) {
      Model.Module m = moduleByName.get(name);
      if (m == null) continue;
      for (Model.SourceSet s : m.sets()) {
        if (!s.test()) roots.addAll(s.roots());
      }
    }
    return roots.stream().filter(Files::isDirectory).map(Path::toFile).toList();
  }

  String describe(Unit u) {
    return (u.module.name() != null ? u.module.name() : rel(u.module.dir())) + " (" + u.set.name() + ")";
  }

  // ── the structure layer ────────────────────────────────────────────────────

  void emitStructure() throws IOException {
    for (Source s : sources) {
      if (s.text == null) s.text = read(s.abs);
      em.emit("file", Main.row(
          "path", s.path, "dir", dirOf(s.path), "package", s.unit.module.name(), "lang", "java",
          "loc", Text.loc(s.text), "sloc", Text.sloc(s.text), "is_test", s.unit.set.test(), "is_decl", false,
          "is_generated", Text.generated(s.path, s.text), "namespace", s.pkg));
    }
    for (Map.Entry<String, String> e : excluded.entrySet()) {
      em.emit("excluded_file", Main.row("path", e.getKey(), "reason", e.getValue(), "detail", null));
    }
    emitProjects();

    // Every declaration's id before any reference names one.
    for (Source s : sources) {
      if (s.cu != null) new Declarations(this, s).run();
    }
    for (Unit u : units) {
      if (u.task == null) continue;
      try {
        u.task.analyze();
      } catch (RuntimeException | StackOverflowError | AssertionError e) {
        Model.warn("javac stopped analysing " + describe(u) + ": " + e + "; its references are partial");
      }
      for (Source s : u.sources) {
        if (s.cu != null) new References(this, u, s).run();
      }
      u.task = null;
      u.fm.close();
      for (Source s : u.sources) s.cu = null;
    }
  }

  private static String read(Path p) throws IOException {
    try {
      return Files.readString(p, StandardCharsets.UTF_8);
    } catch (MalformedInputException e) {
      return Files.readString(p, StandardCharsets.ISO_8859_1);
    }
  }

  private void emitProjects() {
    for (Model.Module m : modules) {
      List<String> files = new ArrayList<>();
      for (Source s : sources) {
        if (s.unit.module == m) files.add(s.path);
      }
      if (!m.dir().toAbsolutePath().normalize().startsWith(root)) continue;
      if (!files.isEmpty()) {
        String id = rel(m.idPath());
        em.emit("project", Main.row("id", id, "dir", rel(m.dir()), "files", files.size(), "strict", false, "module", null, "target", m.release()));
        for (String f : files) em.emit("project_file", Main.row("project", id, "file", f));
      }
      if (m.name() == null) continue;
      em.emit("package", Main.row("name", m.name(), "dir", rel(m.dir()), "version", m.version(), "private", false));
      for (Model.Dep d : m.deps()) {
        em.emit("package_dep", Main.row(
            "package", m.name(), "dep", d.coord(), "kind", depKind(m.tool(), d), "range", d.version() == null ? "" : d.version(),
            "types_for", null, "scope", d.scope()));
      }
    }
  }

  /** The library's four-way split of the build's own word. */
  static String depKind(Model.Tool tool, Model.Dep d) {
    String scope = d.scope() == null ? "" : d.scope();
    if (tool == Model.Tool.GRADLE) {
      if (scope.startsWith("test") || scope.equals("annotationProcessor")) return "dev";
      if (scope.equals("compileOnly")) return "peer";
      return "prod";
    }
    if (d.optional()) return "optional";
    return switch (scope) {
      case "test" -> "dev";
      case "provided" -> "peer";
      default -> "prod";
    };
  }

  // ── symbols ────────────────────────────────────────────────────────────────

  /** The id a declaration at {@code key} gets: the candidate, or it with `@line`, `@line:col`, `#n`. */
  String claim(String candidate, String key, long line, long col) {
    String existing = idByKey.get(key);
    if (existing != null) return existing;
    for (String t : List.of(candidate, candidate + "@" + line, candidate + "@" + line + ":" + col)) {
      if (!keyById.containsKey(t)) return bind(t, key);
    }
    for (int n = 2; ; n++) {
      String t = candidate + "@" + line + ":" + col + "#" + n;
      if (!keyById.containsKey(t)) return bind(t, key);
    }
  }

  private String bind(String id, String key) {
    keyById.put(id, key);
    idByKey.put(key, id);
    return id;
  }

  /** `path#Name` under a file's module, `Container.name` under anything else. */
  static String member(String container, String segment) {
    if (container.endsWith("#<module>")) return container.substring(0, container.length() - "<module>".length()) + segment;
    return container + "." + segment;
  }

  void addSymbol(String id, String name, String kind, String form, Source s, long line, long endLine, String parent,
      boolean exported, String visibility, boolean isStatic, boolean isAbstract, boolean readonly) {
    symbols.putIfAbsent(id, Main.row(
        "id", id, "name", name, "kind", kind, "origin", "project", "file", s.path, "line", line, "end_line", endLine,
        "parent", parent, "package", s.unit.module.name(), "exported", exported, "visibility", visibility,
        "is_static", isStatic, "is_abstract", isAbstract, "is_async", false, "is_generator", false,
        "is_readonly", readonly, "is_optional", false, "is_ambient", false, "form", form));
  }

  void addExternal(String id, String name, String kind, String form, String pkg, String parent, Set<Modifier> mods) {
    if (symbols.containsKey(id)) return;
    String visibility = mods.contains(Modifier.PUBLIC) ? "public" : mods.contains(Modifier.PROTECTED) ? "protected" : mods.contains(Modifier.PRIVATE) ? "private" : kind.equals("namespace") ? null : "package";
    symbols.put(id, Main.row(
        "id", id, "name", name, "kind", kind, "origin", "external", "file", null, "line", null, "end_line", null,
        "parent", parent, "package", pkg, "exported", mods.contains(Modifier.PUBLIC), "visibility", visibility,
        "is_static", mods.contains(Modifier.STATIC), "is_abstract", mods.contains(Modifier.ABSTRACT) && !kind.equals("interface"), "is_async", false, "is_generator", false,
        "is_readonly", mods.contains(Modifier.FINAL) && (kind.equals("property") || kind.equals("enum_member")), "is_optional", false, "is_ambient", true, "form", form));
  }

  void flushSymbols() {
    for (Map<String, Object> r : symbols.values()) em.emit("symbol", r);
  }
}

package codefacts;

import java.io.IOException;
import java.io.StringWriter;
import java.nio.charset.StandardCharsets;
import java.nio.file.FileAlreadyExistsException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HexFormat;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.regex.Matcher;
import java.util.regex.Pattern;
import java.util.stream.Stream;
import javax.tools.JavaCompiler;
import javax.tools.StandardJavaFileManager;
import javax.tools.ToolProvider;
import javax.xml.parsers.DocumentBuilderFactory;
import org.w3c.dom.Document;
import org.w3c.dom.Element;
import org.w3c.dom.Node;

/**
 * The project model, asked of the build: Maven through a core extension that
 * prints its reactor, Gradle through an init script that prints each project's
 * source sets, and a directory of sources with no build as the last resort. A
 * build that cannot answer degrades to its modules' conventional source
 * directories with no classpath, says so on stderr, and never stops extraction.
 */
final class Model {
  enum Tool {
    MAVEN,
    GRADLE,
    PLAIN
  }

  /** A dependency as the build declares it; {@code scope} is the build's own word. */
  record Dep(String coord, String version, String scope, boolean optional) {}

  /** A resolved classpath entry, with the coordinate it resolved as when the build says. */
  record Jar(Path file, String coord) {}

  /** A unit the compiler reads with one classpath: main or test. */
  record SourceSet(String name, boolean test, List<Path> roots, List<Jar> classpath, List<String> modules) {}

  /** A build module: a Maven module, a Gradle project, or a plain directory. */
  record Module(Tool tool, Path buildFile, Path dir, String name, String version, String release, List<SourceSet> sets, List<Dep> deps) {
    /** What the module's `project.id` names: its build file, or its directory. */
    Path idPath() {
      return buildFile != null ? buildFile : dir;
    }
  }

  private final Path resources;
  private final Path cache;

  Model(Path resources, Path cache) {
    this.resources = resources;
    this.cache = cache;
  }

  /** A pom.xml, a build.gradle(.kts) or settings.gradle(.kts), or a directory. */
  List<Module> load(Path target) throws IOException {
    Path abs = target.toAbsolutePath().normalize();
    boolean isDir = Files.isDirectory(abs);
    Path dir = isDir ? abs : abs.getParent();
    String base = isDir ? "" : abs.getFileName().toString();
    List<Module> modules;
    if (base.equals("pom.xml") || (isDir && Files.exists(dir.resolve("pom.xml")))) {
      modules = maven(dir);
    } else if (base.startsWith("build.gradle") || base.startsWith("settings.gradle") || (isDir && gradleBuild(dir))) {
      modules = gradle(dir);
    } else {
      modules = List.of(conventional(Tool.PLAIN, null, dir, null, null, null, List.of(), true));
    }
    List<Module> sorted = new ArrayList<>(modules);
    sorted.sort(Comparator.comparing((Module m) -> m.idPath().toString()));
    return sorted;
  }

  static boolean gradleBuild(Path dir) {
    for (String f : List.of("settings.gradle", "settings.gradle.kts", "build.gradle", "build.gradle.kts")) {
      if (Files.exists(dir.resolve(f))) return true;
    }
    return false;
  }

  /**
   * `src/main/java` and `src/test/java`; a directory holding neither is one main
   * source root, when {@code wholeDir} allows it.
   */
  static Module conventional(Tool tool, Path buildFile, Path dir, String name, String version, String release, List<Dep> deps, boolean wholeDir) {
    Path main = dir.resolve("src/main/java");
    Path test = dir.resolve("src/test/java");
    List<SourceSet> sets = new ArrayList<>();
    if (!wholeDir || Files.isDirectory(main) || Files.isDirectory(test)) {
      sets.add(new SourceSet("main", false, List.of(main), List.of(), List.of()));
      sets.add(new SourceSet("test", true, List.of(test), List.of(), List.of()));
    } else {
      sets.add(new SourceSet("main", false, List.of(dir), List.of(), List.of()));
    }
    return new Module(tool, buildFile, dir, name, version, release, sets, deps);
  }

  // ── running a build ────────────────────────────────────────────────────────

  record Proc(int code, String output) {}

  /** Runs a command to completion; null when it cannot be started at all. */
  static Proc run(Path cwd, List<String> command) {
    Path log = null;
    try {
      log = Files.createTempFile("code-facts-build", ".log");
      Process p = new ProcessBuilder(command).directory(cwd.toFile()).redirectErrorStream(true).redirectOutput(log.toFile()).redirectInput(ProcessBuilder.Redirect.from(nullFile())).start();
      int code = p.waitFor();
      String out = Files.readString(log, StandardCharsets.UTF_8);
      return new Proc(code, out.length() > 4000 ? out.substring(out.length() - 4000) : out);
    } catch (IOException e) {
      return null;
    } catch (InterruptedException e) {
      Thread.currentThread().interrupt();
      return null;
    } finally {
      if (log != null) {
        try {
          Files.deleteIfExists(log);
        } catch (IOException ignored) {
          // a temporary file left behind is no reason to fail
        }
      }
    }
  }

  private static java.io.File nullFile() {
    return new java.io.File(System.getProperty("os.name").startsWith("Windows") ? "NUL" : "/dev/null");
  }

  static void warn(String message) {
    System.err.println("code-facts: " + message);
  }

  // ── Maven ──────────────────────────────────────────────────────────────────

  private static final Pattern MAVEN_HOME = Pattern.compile("(?m)^Maven home: (.+?)\\s*$");

  List<Module> maven(Path dir) throws IOException {
    Path wrapper = dir.resolve("mvnw");
    String mvn = Files.isExecutable(wrapper) ? wrapper.toString() : "mvn";
    Proc version = run(dir, List.of(mvn, "--version"));
    if (version == null || version.code() != 0) {
      return mavenFallback(dir, "`" + mvn + " --version` could not run" + tail(version));
    }
    Matcher m = MAVEN_HOME.matcher(version.output());
    if (!m.find()) return mavenFallback(dir, "`" + mvn + " --version` did not name its Maven home");
    Path extension = mavenExtension(Path.of(m.group(1)), version.output());
    if (extension == null) return mavenFallback(dir, "the Maven model extension could not be built");
    Path out = Files.createTempFile("code-facts-maven", ".json");
    try {
      // The extension writes the reactor and then stops the build, so no plugin runs.
      Proc r = run(dir, List.of(mvn, "-B", "-q", "-f", dir.resolve("pom.xml").toString(), "-Dmaven.ext.class.path=" + extension, "-Dcodefacts.model=" + out, "validate"));
      String json = Files.readString(out, StandardCharsets.UTF_8);
      if (json.isBlank()) return mavenFallback(dir, "Maven did not describe its reactor" + tail(r));
      return modules(Tool.MAVEN, json);
    } finally {
      Files.deleteIfExists(out);
    }
  }

  private static String tail(Proc p) {
    if (p == null) return " (the command was not found)";
    String out = p.output().strip();
    return out.isEmpty() ? " (exit " + p.code() + ")" : " (exit " + p.code() + "):\n" + out;
  }

  /** Compiles the extension against the Maven that will load it, once per Maven. */
  private Path mavenExtension(Path home, String versionText) throws IOException {
    Path source = resources.resolve("maven").resolve("ModelExtension.java");
    Path json = resources.resolve("Json.java");
    String hash = sha256(Files.readAllBytes(source), Files.readAllBytes(json), home.toString().getBytes(StandardCharsets.UTF_8), versionText.getBytes(StandardCharsets.UTF_8));
    Path built = cache.resolve("maven-ext-" + hash);
    if (Files.isDirectory(built)) return built;
    List<String> jars = new ArrayList<>();
    for (String sub : List.of("lib", "boot")) {
      Path d = home.resolve(sub);
      if (!Files.isDirectory(d)) continue;
      try (Stream<Path> s = Files.list(d)) {
        s.filter(p -> p.toString().endsWith(".jar")).map(Path::toString).sorted().forEach(jars::add);
      }
    }
    Files.createDirectories(cache);
    Path partial = cache.resolve("maven-ext-" + hash + "." + ProcessHandle.current().pid() + ".partial");
    JavaCompiler javac = ToolProvider.getSystemJavaCompiler();
    if (javac == null) return null;
    StringWriter errors = new StringWriter();
    boolean ok;
    try (StandardJavaFileManager fm = javac.getStandardFileManager(null, null, StandardCharsets.UTF_8)) {
      List<String> options = List.of("--release", "17", "-proc:none", "-nowarn", "-d", partial.toString(), "-cp", String.join(java.io.File.pathSeparator, jars));
      ok = javac.getTask(errors, fm, null, options, null, fm.getJavaFileObjects(source, json)).call();
    }
    if (!ok) {
      warn("building the Maven model extension against " + home + " failed:\n" + errors);
      return null;
    }
    Path index = partial.resolve("META-INF/sisu/javax.inject.Named");
    Files.createDirectories(index.getParent());
    Files.writeString(index, "codefacts.ModelExtension\n");
    try {
      Files.move(partial, built, StandardCopyOption.ATOMIC_MOVE);
    } catch (FileAlreadyExistsException | java.nio.file.DirectoryNotEmptyException e) {
      deleteTree(partial); // another run built it first
    }
    return built;
  }

  /** Without Maven: the reactor read from the poms, each module's conventional roots, no classpath. */
  private List<Module> mavenFallback(Path dir, String why) {
    warn(why + "; reading each Maven module's src/main/java and src/test/java without a classpath, so names from dependencies are unresolved");
    List<Module> out = new ArrayList<>();
    readPom(dir.resolve("pom.xml"), null, out, new LinkedHashSet<>());
    if (out.isEmpty()) out.add(conventional(Tool.MAVEN, dir.resolve("pom.xml"), dir, null, null, null, List.of(), false));
    return out;
  }

  private void readPom(Path pom, String parentGroup, List<Module> out, Set<Path> seen) {
    Path abs = pom.toAbsolutePath().normalize();
    if (!seen.add(abs) || !Files.isRegularFile(abs)) return;
    Element project;
    try {
      DocumentBuilderFactory f = DocumentBuilderFactory.newInstance();
      f.setNamespaceAware(false);
      Document doc = f.newDocumentBuilder().parse(abs.toFile());
      project = doc.getDocumentElement();
    } catch (Exception e) {
      warn("cannot read " + abs + ": " + e.getMessage());
      return;
    }
    Element parent = child(project, "parent");
    String group = text(project, "groupId");
    if (group == null && parent != null) group = text(parent, "groupId");
    if (group == null) group = parentGroup;
    String artifact = text(project, "artifactId");
    String version = text(project, "version");
    if (version == null && parent != null) version = text(parent, "version");
    String release = null;
    Element props = child(project, "properties");
    if (props != null) {
      release = text(props, "maven.compiler.release");
      if (release == null) release = text(props, "maven.compiler.source");
    }
    List<Dep> deps = new ArrayList<>();
    Element depsEl = child(project, "dependencies");
    if (depsEl != null) {
      for (Element d : children(depsEl, "dependency")) {
        String scope = text(d, "scope");
        deps.add(new Dep(text(d, "groupId") + ":" + text(d, "artifactId"), text(d, "version"), scope == null ? "compile" : scope, "true".equals(text(d, "optional"))));
      }
    }
    Path dir = abs.getParent();
    out.add(conventional(Tool.MAVEN, abs, dir, artifact == null ? null : group + ":" + artifact, version, release, deps, false));
    for (String listName : List.of("modules", "subprojects")) {
      Element list = child(project, listName);
      if (list == null) continue;
      for (Node n = list.getFirstChild(); n != null; n = n.getNextSibling()) {
        if (!(n instanceof Element e)) continue;
        Path next = dir.resolve(e.getTextContent().trim());
        readPom(Files.isDirectory(next) ? next.resolve("pom.xml") : next, group, out, seen);
      }
    }
  }

  private static Element child(Element e, String name) {
    for (Node n = e.getFirstChild(); n != null; n = n.getNextSibling()) {
      if (n instanceof Element c && c.getTagName().equals(name)) return c;
    }
    return null;
  }

  private static List<Element> children(Element e, String name) {
    List<Element> out = new ArrayList<>();
    for (Node n = e.getFirstChild(); n != null; n = n.getNextSibling()) {
      if (n instanceof Element c && c.getTagName().equals(name)) out.add(c);
    }
    return out;
  }

  private static String text(Element e, String name) {
    Element c = child(e, name);
    return c == null ? null : c.getTextContent().trim();
  }

  // ── Gradle ─────────────────────────────────────────────────────────────────

  List<Module> gradle(Path dir) throws IOException {
    Path settingsDir = dir;
    for (Path d = dir; d != null; d = d.getParent()) {
      if (Files.exists(d.resolve("settings.gradle")) || Files.exists(d.resolve("settings.gradle.kts"))) {
        settingsDir = d;
        break;
      }
    }
    Path wrapper = settingsDir.resolve("gradlew");
    String gradle = Files.isExecutable(wrapper) ? wrapper.toString() : "gradle";
    Path out = Files.createTempDirectory("code-facts-gradle");
    try {
      Proc r = run(settingsDir, List.of(gradle, "-q", "--console=plain", "--no-configuration-cache", "--warning-mode=none", "--init-script", resources.resolve("model.gradle").toString(), "-Pcodefacts.model=" + out, "codeFactsModel"));
      List<Module> modules = new ArrayList<>();
      try (Stream<Path> s = Files.list(out)) {
        for (Path f : s.filter(p -> p.toString().endsWith(".json")).sorted().toList()) {
          modules.addAll(modules(Tool.GRADLE, "{\"modules\":[" + Files.readString(f, StandardCharsets.UTF_8) + "]}"));
        }
      }
      modules.removeIf(m -> !m.dir().startsWith(dir));
      if (modules.isEmpty()) return gradleFallback(dir, "Gradle did not describe a Java project" + tail(r));
      return modules;
    } finally {
      deleteTree(out);
    }
  }

  /** Without Gradle: every directory holding a build script, with conventional roots. */
  private List<Module> gradleFallback(Path dir, String why) throws IOException {
    warn(why + "; reading each Gradle project's src/main/java and src/test/java without a classpath, so names from dependencies are unresolved");
    List<Module> out = new ArrayList<>();
    try (Stream<Path> s = Files.walk(dir)) {
      for (Path d : s.filter(Files::isDirectory).filter(p -> !skippedDir(dir, p)).sorted().toList()) {
        Path script = Files.exists(d.resolve("build.gradle")) ? d.resolve("build.gradle") : Files.exists(d.resolve("build.gradle.kts")) ? d.resolve("build.gradle.kts") : null;
        if (script == null && !d.equals(dir)) continue;
        String rel = dir.relativize(d).toString().replace(java.io.File.separatorChar, ':');
        out.add(conventional(Tool.GRADLE, script, d, rel.isEmpty() ? ":" : ":" + rel, null, null, List.of(), false));
      }
    }
    return out;
  }

  static boolean skippedDir(Path root, Path d) {
    for (Path part : root.relativize(d)) {
      String n = part.toString();
      if (n.equals("build") || n.equals("target") || n.equals("node_modules") || n.startsWith(".")) return true;
    }
    return false;
  }

  // ── the model as JSON ──────────────────────────────────────────────────────

  @SuppressWarnings("unchecked")
  static List<Module> modules(Tool tool, String json) {
    Map<String, Object> root = (Map<String, Object>) Json.parse(json);
    List<Module> out = new ArrayList<>();
    for (Object o : (List<Object>) root.get("modules")) {
      Map<String, Object> m = (Map<String, Object>) o;
      List<SourceSet> sets = new ArrayList<>();
      for (Object so : (List<Object>) m.get("sets")) {
        Map<String, Object> s = (Map<String, Object>) so;
        List<Jar> classpath = new ArrayList<>();
        for (Object jo : (List<Object>) s.get("classpath")) {
          Map<String, Object> j = (Map<String, Object>) jo;
          classpath.add(new Jar(Path.of((String) j.get("file")), (String) j.get("coord")));
        }
        List<Path> roots = ((List<Object>) s.get("roots")).stream().map(r -> Path.of((String) r)).toList();
        List<String> modules = ((List<Object>) s.get("modules")).stream().map(String.class::cast).toList();
        sets.add(new SourceSet((String) s.get("name"), Boolean.TRUE.equals(s.get("test")), roots, classpath, modules));
      }
      List<Dep> deps = new ArrayList<>();
      for (Object dobj : (List<Object>) m.get("deps")) {
        Map<String, Object> d = (Map<String, Object>) dobj;
        deps.add(new Dep((String) d.get("coord"), (String) d.get("version"), (String) d.get("scope"), Boolean.TRUE.equals(d.get("optional"))));
      }
      String build = (String) m.get("buildFile");
      out.add(new Module(tool, build == null ? null : Path.of(build), Path.of((String) m.get("dir")), (String) m.get("name"), (String) m.get("version"), (String) m.get("release"), sets, deps));
    }
    return out;
  }

  static String sha256(byte[]... parts) {
    try {
      MessageDigest md = MessageDigest.getInstance("SHA-256");
      for (byte[] p : parts) {
        md.update(p);
        md.update((byte) 0);
      }
      return HexFormat.of().formatHex(md.digest()).substring(0, 16);
    } catch (NoSuchAlgorithmException e) {
      throw new IllegalStateException(e);
    }
  }

  static void deleteTree(Path p) {
    if (!Files.exists(p)) return;
    try (Stream<Path> s = Files.walk(p)) {
      for (Path q : s.sorted(Comparator.reverseOrder()).toList()) Files.deleteIfExists(q);
    } catch (IOException ignored) {
      // a temporary directory left behind is no reason to fail
    }
  }
}

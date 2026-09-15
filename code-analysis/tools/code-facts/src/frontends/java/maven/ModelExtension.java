package codefacts;

import java.io.File;
import java.io.StringReader;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.regex.Matcher;
import java.util.regex.Pattern;
import javax.inject.Inject;
import javax.inject.Named;
import javax.inject.Singleton;
import javax.xml.parsers.DocumentBuilderFactory;
import org.apache.maven.AbstractMavenLifecycleParticipant;
import org.apache.maven.MavenExecutionException;
import org.apache.maven.execution.MavenSession;
import org.apache.maven.model.Dependency;
import org.apache.maven.model.Plugin;
import org.apache.maven.model.PluginExecution;
import org.apache.maven.project.DefaultDependencyResolutionRequest;
import org.apache.maven.project.DependencyResolutionException;
import org.apache.maven.project.DependencyResolutionResult;
import org.apache.maven.project.MavenProject;
import org.apache.maven.project.ProjectDependenciesResolver;
import org.eclipse.aether.RepositorySystem;
import org.eclipse.aether.artifact.Artifact;
import org.eclipse.aether.artifact.DefaultArtifact;
import org.eclipse.aether.collection.CollectRequest;
import org.eclipse.aether.resolution.ArtifactResult;
import org.eclipse.aether.resolution.DependencyRequest;
import org.eclipse.aether.resolution.DependencyResult;
import org.w3c.dom.Element;
import org.w3c.dom.Node;
import org.xml.sax.InputSource;
import org.eclipse.aether.graph.DependencyNode;

/**
 * code-facts' view of a Maven reactor, loaded with {@code -Dmaven.ext.class.path}:
 * once the reactor is read it writes every module's source roots, release level,
 * declared dependencies and resolved classpaths to {@code -Dcodefacts.model}, then
 * stops the build, so no plugin runs. Compiled on first use against the Maven
 * that loads it.
 */
@Named("code-facts-model")
@Singleton
public final class ModelExtension extends AbstractMavenLifecycleParticipant {
  private static final Set<String> COMPILE_SCOPES = Set.of("compile", "provided", "system");
  private static final Pattern RELEASE = Pattern.compile("<release>\\s*([^<\\s]+)\\s*</release>");
  private static final Pattern SOURCE = Pattern.compile("<source>\\s*([^<\\s]+)\\s*</source>");

  /** Plugins that generate sources in the build's own generate-sources step. */
  private static final Set<String> CODEGEN = Set.of(
      "build-helper-maven-plugin", "protobuf-maven-plugin", "protoc-jar-maven-plugin", "openapi-generator-maven-plugin",
      "swagger-codegen-maven-plugin", "jooq-codegen-maven", "antlr4-maven-plugin", "avro-maven-plugin", "jaxb2-maven-plugin",
      "maven-jaxb2-plugin", "cxf-codegen-plugin", "javacc-maven-plugin", "jsonschema2pojo-maven-plugin", "querydsl-maven-plugin");

  private final ProjectDependenciesResolver resolver;
  private final RepositorySystem repositories;

  @Inject
  public ModelExtension(ProjectDependenciesResolver resolver, RepositorySystem repositories) {
    this.resolver = resolver;
    this.repositories = repositories;
  }

  @Override
  public void afterProjectsRead(MavenSession session) throws MavenExecutionException {
    String out = session.getUserProperties().getProperty("codefacts.model");
    if (out == null) return;
    Set<String> reactor = new HashSet<>();
    for (MavenProject p : session.getProjects()) reactor.add(p.getGroupId() + ":" + p.getArtifactId());
    List<Object> modules = new ArrayList<>();
    for (MavenProject p : session.getProjects()) modules.add(module(session, p, reactor));
    Map<String, Object> model = new LinkedHashMap<>();
    model.put("modules", modules);
    try {
      Files.writeString(Path.of(out), Json.write(model), StandardCharsets.UTF_8);
    } catch (Exception e) {
      throw new MavenExecutionException("code-facts: cannot write the project model: " + e.getMessage(), session.getRequest().getPom());
    }
    throw new MavenExecutionException("code-facts: project model written; the build stops here", session.getRequest().getPom());
  }

  private Map<String, Object> module(MavenSession session, MavenProject p, Set<String> reactor) {
    Map<String, Object> m = new LinkedHashMap<>();
    m.put("buildFile", p.getFile() == null ? null : p.getFile().getAbsolutePath());
    m.put("dir", p.getBasedir().getAbsolutePath());
    m.put("name", p.getGroupId() + ":" + p.getArtifactId());
    m.put("version", p.getVersion());
    m.put("release", release(p));
    m.put("buildDir", new File(p.getBuild().getDirectory()).getAbsolutePath());
    m.put("codegen", codegen(p));

    List<Map<String, Object>> main = new ArrayList<>();
    List<Map<String, Object>> test = new ArrayList<>();
    Set<String> mainModules = new LinkedHashSet<>();
    Set<String> testModules = new LinkedHashSet<>();
    DefaultDependencyResolutionRequest request = new DefaultDependencyResolutionRequest(p, session.getRepositorySession());
    // A sibling module is read from its sources, not a jar that may not be built.
    request.setResolutionFilter((node, parents) -> node.getArtifact() == null || !reactor.contains(coord(node.getArtifact())));
    DependencyResolutionResult result;
    try {
      result = resolver.resolve(request);
    } catch (DependencyResolutionException e) {
      System.err.println("code-facts: some dependencies of " + p.getId() + " did not resolve: " + e.getMessage());
      result = e.getResult();
    }
    if (result != null) {
      for (org.eclipse.aether.graph.Dependency d : result.getDependencies()) {
        File file = d.getArtifact().getFile();
        if (file == null) continue;
        Map<String, Object> jar = new LinkedHashMap<>();
        jar.put("file", file.getAbsolutePath());
        jar.put("coord", coord(d.getArtifact()));
        if (COMPILE_SCOPES.contains(d.getScope())) main.add(jar);
        test.add(jar);
      }
      if (result.getDependencyGraph() != null) siblings(result.getDependencyGraph(), reactor, mainModules, testModules, new HashSet<>());
    }
    m.put("sets", List.of(
        set("main", false, p.getCompileSourceRoots(), main, mainModules, compiler(session, p, false)),
        set("test", true, p.getTestCompileSourceRoots(), test, testModules, compiler(session, p, true))));

    List<Object> deps = new ArrayList<>();
    for (Dependency d : p.getDependencies()) {
      Map<String, Object> dep = new LinkedHashMap<>();
      dep.put("coord", d.getGroupId() + ":" + d.getArtifactId());
      dep.put("version", d.getVersion());
      dep.put("scope", d.getScope() == null ? "compile" : d.getScope());
      dep.put("optional", d.isOptional());
      deps.add(dep);
    }
    m.put("deps", deps);
    return m;
  }

  private static void siblings(DependencyNode node, Set<String> reactor, Set<String> main, Set<String> test, Set<DependencyNode> seen) {
    for (DependencyNode child : node.getChildren()) {
      if (!seen.add(child)) continue;
      if (child.getArtifact() != null && child.getDependency() != null && reactor.contains(coord(child.getArtifact()))) {
        String name = coord(child.getArtifact());
        if (COMPILE_SCOPES.contains(child.getDependency().getScope())) main.add(name);
        test.add(name);
      }
      siblings(child, reactor, main, test, seen);
    }
  }

  private static Map<String, Object> set(String name, boolean isTest, List<String> roots, List<Map<String, Object>> classpath, Set<String> modules, Map<String, Object> compiler) {
    Map<String, Object> s = new LinkedHashMap<>();
    s.put("name", name);
    s.put("test", isTest);
    s.put("roots", new ArrayList<>(roots));
    s.put("classpath", classpath);
    s.put("modules", new ArrayList<>(modules));
    s.put("compiler", compiler);
    return s;
  }

  /**
   * How maven-compiler-plugin runs javac for main or test: its configuration, with
   * the default-compile or default-testCompile execution's over it.
   */
  private Map<String, Object> compiler(MavenSession session, MavenProject p, boolean test) {
    List<Element> layers = new ArrayList<>();
    Plugin plugin = p.getPlugin("org.apache.maven.plugins:maven-compiler-plugin");
    if (plugin != null) {
      for (PluginExecution e : plugin.getExecutions()) {
        if (e.getId().equals(test ? "default-testCompile" : "default-compile")) addLayer(layers, e.getConfiguration());
      }
      addLayer(layers, plugin.getConfiguration());
    }
    List<String> args = new ArrayList<>();
    Element compilerArgs = first(layers, "compilerArgs");
    if (compilerArgs != null) for (Element a : children(compilerArgs, "arg")) args.add(a.getTextContent().trim());
    String one = value(layers, "compilerArgument");
    if (one != null && !one.isBlank()) args.addAll(List.of(one.trim().split("\\s+")));
    if ("true".equals(value(layers, "enablePreview"))) args.add("--enable-preview");
    if ("true".equals(value(layers, "parameters"))) args.add("-parameters");
    String encoding = value(layers, "encoding");
    if (encoding == null) encoding = p.getProperties().getProperty("project.build.sourceEncoding");
    List<String> processors = new ArrayList<>();
    Element named = first(layers, "annotationProcessors");
    if (named != null) for (Element a : children(named, "annotationProcessor")) processors.add(a.getTextContent().trim());
    Element paths = first(layers, "annotationProcessorPaths");
    String generated = value(layers, test ? "generatedTestSourcesDirectory" : "generatedSourcesDirectory");
    File generatedDir = generated != null ? new File(generated) : new File(p.getBuild().getDirectory(), test ? "generated-test-sources/test-annotations" : "generated-sources/annotations");
    if (!generatedDir.isAbsolute()) generatedDir = new File(p.getBasedir(), generatedDir.getPath());

    Map<String, Object> c = new LinkedHashMap<>();
    c.put("processorPath", paths == null ? null : processorPath(session, p, children(paths, "path")));
    c.put("processors", processors);
    c.put("proc", value(layers, "proc"));
    c.put("encoding", encoding);
    c.put("args", args);
    c.put("generatedDir", generatedDir.getAbsolutePath());
    return c;
  }

  /** The processor path's artifacts, resolved with their dependencies as the compiler plugin resolves them. */
  private List<String> processorPath(MavenSession session, MavenProject p, List<Element> paths) {
    CollectRequest collect = new CollectRequest();
    for (Element e : paths) {
      String group = text(e, "groupId");
      String artifact = text(e, "artifactId");
      String version = text(e, "version");
      if (version == null && p.getDependencyManagement() != null) {
        for (Dependency d : p.getDependencyManagement().getDependencies()) {
          if (d.getGroupId().equals(group) && d.getArtifactId().equals(artifact)) version = d.getVersion();
        }
      }
      if (group == null || artifact == null || version == null) {
        System.err.println("code-facts: an annotation processor path of " + p.getId() + " names no version: " + group + ":" + artifact);
        continue;
      }
      String classifier = text(e, "classifier");
      String type = text(e, "type");
      collect.addDependency(new org.eclipse.aether.graph.Dependency(new DefaultArtifact(group, artifact, classifier == null ? "" : classifier, type == null ? "jar" : type, version), "runtime"));
    }
    collect.setRepositories(p.getRemoteProjectRepositories());
    List<String> out = new ArrayList<>();
    DependencyResult result;
    try {
      result = repositories.resolveDependencies(session.getRepositorySession(), new DependencyRequest(collect, null));
    } catch (org.eclipse.aether.resolution.DependencyResolutionException e) {
      System.err.println("code-facts: annotation processors of " + p.getId() + " did not resolve: " + e.getMessage());
      result = e.getResult();
    }
    if (result != null) {
      for (ArtifactResult r : result.getArtifactResults()) {
        if (r.getArtifact() != null && r.getArtifact().getFile() != null) out.add(r.getArtifact().getFile().getAbsolutePath());
      }
    }
    return out;
  }

  /** Whether a plugin of the build generates sources: a known generator, or an execution bound to a source-generating phase. */
  private static boolean codegen(MavenProject p) {
    for (Plugin plugin : p.getBuildPlugins()) {
      if (plugin.getArtifactId().equals("maven-compiler-plugin")) continue;
      if (CODEGEN.contains(plugin.getArtifactId())) return true;
      for (PluginExecution e : plugin.getExecutions()) {
        String phase = e.getPhase();
        if ("generate-sources".equals(phase) || "generate-test-sources".equals(phase)) return true;
      }
    }
    return false;
  }

  private static void addLayer(List<Element> layers, Object config) {
    if (config == null) return;
    try {
      DocumentBuilderFactory f = DocumentBuilderFactory.newInstance();
      layers.add(f.newDocumentBuilder().parse(new InputSource(new StringReader(config.toString()))).getDocumentElement());
    } catch (Exception e) {
      System.err.println("code-facts: cannot read a maven-compiler-plugin configuration: " + e.getMessage());
    }
  }

  private static Element first(List<Element> layers, String name) {
    for (Element layer : layers) {
      for (Element c : children(layer, name)) return c;
    }
    return null;
  }

  private static String value(List<Element> layers, String name) {
    Element e = first(layers, name);
    return e == null ? null : e.getTextContent().trim();
  }

  private static List<Element> children(Element e, String name) {
    List<Element> out = new ArrayList<>();
    for (Node n = e.getFirstChild(); n != null; n = n.getNextSibling()) {
      if (n instanceof Element c && c.getTagName().equals(name)) out.add(c);
    }
    return out;
  }

  private static String text(Element e, String name) {
    List<Element> c = children(e, name);
    return c.isEmpty() ? null : c.get(0).getTextContent().trim();
  }

  private static String coord(Artifact a) {
    return a.getGroupId() + ":" + a.getArtifactId();
  }

  /** The level the compiler plugin compiles for: its configuration, then the properties it reads. */
  private static String release(MavenProject p) {
    Plugin compiler = p.getPlugin("org.apache.maven.plugins:maven-compiler-plugin");
    Object config = compiler == null ? null : compiler.getConfiguration();
    if (config != null) {
      String xml = config.toString();
      Matcher r = RELEASE.matcher(xml);
      if (r.find()) return r.group(1);
      Matcher s = SOURCE.matcher(xml);
      if (s.find()) return s.group(1);
    }
    String release = p.getProperties().getProperty("maven.compiler.release");
    return release != null ? release : p.getProperties().getProperty("maven.compiler.source");
  }
}

package codefacts;

import java.io.File;
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
import org.apache.maven.AbstractMavenLifecycleParticipant;
import org.apache.maven.MavenExecutionException;
import org.apache.maven.execution.MavenSession;
import org.apache.maven.model.Dependency;
import org.apache.maven.model.Plugin;
import org.apache.maven.project.DefaultDependencyResolutionRequest;
import org.apache.maven.project.DependencyResolutionException;
import org.apache.maven.project.DependencyResolutionResult;
import org.apache.maven.project.MavenProject;
import org.apache.maven.project.ProjectDependenciesResolver;
import org.eclipse.aether.artifact.Artifact;
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

  private final ProjectDependenciesResolver resolver;

  @Inject
  public ModelExtension(ProjectDependenciesResolver resolver) {
    this.resolver = resolver;
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
    m.put("sets", List.of(set("main", false, p.getCompileSourceRoots(), main, mainModules), set("test", true, p.getTestCompileSourceRoots(), test, testModules)));

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

  private static Map<String, Object> set(String name, boolean isTest, List<String> roots, List<Map<String, Object>> classpath, Set<String> modules) {
    Map<String, Object> s = new LinkedHashMap<>();
    s.put("name", name);
    s.put("test", isTest);
    s.put("roots", new ArrayList<>(roots));
    s.put("classpath", classpath);
    s.put("modules", new ArrayList<>(modules));
    return s;
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

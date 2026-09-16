package codefacts;

import com.sun.source.tree.BlockTree;
import com.sun.source.tree.ClassTree;
import com.sun.source.tree.CompilationUnitTree;
import com.sun.source.tree.LambdaExpressionTree;
import com.sun.source.tree.MethodTree;
import com.sun.source.tree.Tree;
import com.sun.source.tree.VariableTree;
import com.sun.source.util.DocTrees;
import com.sun.source.util.TreePath;
import java.io.IOException;
import java.net.URI;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import javax.lang.model.element.Element;
import javax.lang.model.element.ElementKind;
import javax.lang.model.element.ExecutableElement;
import javax.lang.model.element.Modifier;
import javax.lang.model.element.ModuleElement;
import javax.lang.model.element.PackageElement;
import javax.lang.model.element.TypeElement;
import javax.lang.model.element.VariableElement;
import javax.lang.model.type.DeclaredType;
import javax.lang.model.type.TypeKind;
import javax.lang.model.type.TypeMirror;
import javax.lang.model.util.Elements;
import javax.lang.model.util.Types;
import javax.tools.JavaFileManager;
import javax.tools.JavaFileObject;
import javax.tools.StandardLocation;

/**
 * What an element is, in one analysed source set: its id — a project
 * declaration's, found by its position, or an external one, `ext:<package>#Outer.member`
 * — the file declaring it, and where an outside element comes from.
 */
final class Names {
  final Extractor x;
  final Extractor.Unit u;
  final DocTrees trees;
  final Elements elements;
  final Types types;

  private final Map<TypeElement, Set<Element>> members = new HashMap<>();
  private final Map<TypeElement, Boolean> functional = new HashMap<>();
  private final Map<TypeElement, Extractor.Source> declaredIn = new HashMap<>();

  Names(Extractor x, Extractor.Unit u) {
    this.x = x;
    this.u = u;
    this.trees = DocTrees.instance(u.task);
    this.elements = u.task.getElements();
    this.types = u.task.getTypes();
  }

  /** Its project id, or an external id; a package's own symbol. */
  String idOf(Element e) {
    if (e instanceof PackageElement p) {
      String id = x.packageIds.get(p.getQualifiedName().toString());
      return id != null ? id : externalPackage(p);
    }
    String id = projectId(e);
    return id != null ? id : externalId(e);
  }

  /** The id of the project declaration an element is, found by its position; null outside the root. */
  String projectId(Element e) {
    TreePath p = trees.getPath(e);
    if (p == null) return null;
    CompilationUnitTree c = p.getCompilationUnit();
    Extractor.Source src = x.byAbs.get(abs(c));
    if (src == null) return null;
    String id = x.idByKey.get(Extractor.key(src.abs, trees.getSourcePositions().getStartPosition(c, p.getLeaf()), p.getLeaf()));
    if (id == null) return null;
    Element declared = trees.getElement(p);
    if (declared != null && !declared.equals(e) && declared.getKind() == ElementKind.RECORD) {
      // An accessor the compiler writes is its component's.
      String component = id + "." + e.getSimpleName();
      if (x.symbols.containsKey(component)) return component;
    }
    return id; // a member the compiler writes (a default constructor, an enum's values()) is its type's
  }

  String externalId(Element e) {
    if (e instanceof TypeElement t) {
      String pkg = elements.getPackageOf(t).getQualifiedName().toString();
      String qualified = t.getQualifiedName().toString();
      String nested = pkg.isEmpty() || !qualified.startsWith(pkg + ".") ? qualified : qualified.substring(pkg.length() + 1);
      String id = "ext:" + pkg + "#" + nested;
      String parent = t.getEnclosingElement() instanceof TypeElement outer ? idOf(outer) : null;
      x.addExternal(id, t.getSimpleName().toString(), kindOf(t), formOf(t), packageOf(t), parent, t.getModifiers());
      return id;
    }
    if (e.getEnclosingElement() instanceof TypeElement owner) {
      String parent = idOf(owner);
      // A member the compiler writes into a project type — a default constructor, an enum's values() — is that type's.
      Map<String, Object> ownerRow = x.symbols.get(parent);
      if (ownerRow != null && "project".equals(ownerRow.get("origin"))) return parent;
      String segment = e.getKind() == ElementKind.CONSTRUCTOR ? "constructor" : e.getSimpleName().toString();
      String id = parent + "." + segment;
      x.addExternal(id, segment, kindOf(e), formOf(e), packageOf(owner), parent, e.getModifiers());
      return id;
    }
    return null;
  }

  String externalPackage(PackageElement p) {
    String id = "ext:" + p.getQualifiedName() + "#<package>";
    x.addExternal(id, p.getQualifiedName().toString(), "namespace", null, packageOf(p), null, Set.of());
    return id;
  }

  static String kindOf(Element e) {
    return switch (e.getKind()) {
      case CLASS, ENUM, RECORD -> "class";
      case INTERFACE, ANNOTATION_TYPE -> "interface";
      case METHOD -> "method";
      case CONSTRUCTOR -> "constructor";
      case FIELD, RECORD_COMPONENT -> "property";
      case ENUM_CONSTANT -> "enum_member";
      case STATIC_INIT, INSTANCE_INIT -> "static_block";
      default -> "unknown";
    };
  }

  static String formOf(Element e) {
    return switch (e.getKind()) {
      case ENUM -> "enum";
      case RECORD -> "record";
      case ANNOTATION_TYPE -> "annotation";
      case RECORD_COMPONENT -> "record_component";
      default -> null;
    };
  }

  /**
   * The distribution unit an element outside the root comes from: the coordinate
   * of the jar holding it, else the module it is in. The jar comes first because
   * a dependency of a modular project is a *named module* as well as a jar, and
   * the coordinate is what the build declared and what `packages.dl` joins on.
   */
  String packageOf(Element e) {
    TypeElement t = e instanceof PackageElement p ? firstType(p) : topLevel(e);
    if (t != null && declaredIn(t) != null) return null;
    String coord = t == null ? null : coordinateOf(t);
    if (coord != null) return coord;
    ModuleElement m = elements.getModuleOf(e);
    return m != null && !m.isUnnamed() ? m.getQualifiedName().toString() : null;
  }

  /** From the JDK's own image: in a named module with no jar behind it. */
  boolean jdk(Element e) {
    ModuleElement m = elements.getModuleOf(e);
    if (m == null || m.isUnnamed()) return false;
    TypeElement t = e instanceof PackageElement p ? firstType(p) : topLevel(e);
    if (t == null) return true;
    return declaredIn(t) == null && jarOf(t) == null;
  }

  private String coordinateOf(TypeElement t) {
    Path jar = jarOf(t);
    return jar == null ? null : coordinate(jar);
  }

  private static TypeElement firstType(PackageElement p) {
    for (Element e : p.getEnclosedElements()) {
      if (e instanceof TypeElement t) return t;
    }
    return null;
  }

  /** The jar or directory a type is read from: the class path, else its own module on the module path. */
  private Path jarOf(TypeElement t) {
    Path onClassPath = fileOf(StandardLocation.CLASS_PATH, t);
    if (onClassPath != null) return onClassPath;
    ModuleElement m = elements.getModuleOf(t);
    if (m == null || m.isUnnamed()) return null;
    try {
      JavaFileManager.Location loc = u.fm.getLocationForModule(StandardLocation.MODULE_PATH, m.getQualifiedName().toString());
      return loc == null ? null : fileOf(loc, t);
    } catch (IOException | IllegalArgumentException e) {
      return null;
    }
  }

  private Path fileOf(JavaFileManager.Location where, TypeElement t) {
    try {
      JavaFileObject fo = u.fm.getJavaFileForInput(where, elements.getBinaryName(t).toString(), JavaFileObject.Kind.CLASS);
      if (fo == null) return null;
      URI uri = fo.toUri();
      String text = uri.toString();
      if (text.startsWith("jar:")) {
        int bang = text.indexOf("!/");
        return Path.of(URI.create(text.substring(4, bang < 0 ? text.length() : bang))).toAbsolutePath().normalize();
      }
      return "file".equals(uri.getScheme()) ? Path.of(uri) : null;
    } catch (IOException | IllegalArgumentException e) {
      return null;
    }
  }

  /** The coordinate the build resolved a jar as; else read off a Maven or Gradle cache path. */
  private String coordinate(Path jar) {
    String known = u.coords.get(jar);
    if (known != null) return known;
    List<String> parts = new ArrayList<>();
    for (Path p : jar) parts.add(p.toString());
    int repo = parts.lastIndexOf("repository");
    if (repo >= 0 && parts.size() - repo >= 5) {
      String artifact = parts.get(parts.size() - 3);
      return String.join(".", parts.subList(repo + 1, parts.size() - 3)) + ":" + artifact;
    }
    int gradle = parts.lastIndexOf("files-2.1");
    if (gradle >= 0 && parts.size() - gradle >= 5) return parts.get(gradle + 1) + ":" + parts.get(gradle + 2);
    return null;
  }

  /** The project file declaring a top-level type; null outside the root. */
  Extractor.Source declaredIn(TypeElement top) {
    if (top == null) return null;
    return declaredIn.computeIfAbsent(top, t -> {
      TreePath p = trees.getPath(t);
      return p == null ? null : x.byAbs.get(abs(p.getCompilationUnit()));
    });
  }

  Set<Element> membersOf(TypeElement t) {
    return members.computeIfAbsent(t, k -> new HashSet<>(elements.getAllMembers(k)));
  }

  /**
   * The innermost enclosing declaration with a name — what a reference is from:
   * a method, constructor, lambda, class, field (for its initializer) or
   * initializer block, else the file's `<module>`.
   */
  String owner(Extractor.Source s, TreePath path) {
    for (TreePath p = path.getParentPath(); p != null; p = p.getParentPath()) {
      Tree t = p.getLeaf();
      Tree parent = p.getParentPath() == null ? null : p.getParentPath().getLeaf();
      boolean named = t instanceof MethodTree || t instanceof LambdaExpressionTree || t instanceof ClassTree
          || (t instanceof VariableTree || t instanceof BlockTree) && parent instanceof ClassTree;
      if (!named) continue;
      String id = x.idByKey.get(Extractor.key(s.abs, trees.getSourcePositions().getStartPosition(s.cu, t), t));
      if (id != null) return id;
    }
    return s.path + "#<module>";
  }

  /** A functional interface: exactly one abstract method that is not one of Object's. */
  boolean functional(TypeMirror t) {
    if (!(t instanceof DeclaredType dt) || !(dt.asElement() instanceof TypeElement te) || te.getKind() != ElementKind.INTERFACE) return false;
    return functional.computeIfAbsent(te, k -> {
      TypeElement object = elements.getTypeElement("java.lang.Object");
      int abstracts = 0;
      for (Element m : elements.getAllMembers(k)) {
        if (!(m instanceof ExecutableElement ex) || !ex.getModifiers().contains(Modifier.ABSTRACT)) continue;
        boolean objects = false;
        if (object != null) {
          for (Element om : object.getEnclosedElements()) {
            if (om instanceof ExecutableElement o && o.getModifiers().contains(Modifier.PUBLIC) && elements.overrides(ex, o, k)) objects = true;
          }
        }
        if (!objects) abstracts++;
      }
      return abstracts == 1;
    });
  }

  /** A `Future` or a `CompletionStage`. */
  boolean promise(TypeMirror t) {
    if (t.getKind() != TypeKind.DECLARED) return false;
    for (String name : List.of("java.util.concurrent.Future", "java.util.concurrent.CompletionStage")) {
      TypeElement te = elements.getTypeElement(name);
      if (te != null && types.isAssignable(types.erasure(t), types.erasure(te.asType()))) return true;
    }
    return false;
  }

  static Path abs(CompilationUnitTree c) {
    return Path.of(c.getSourceFile().toUri()).toAbsolutePath().normalize();
  }

  static TypeElement topLevel(Element e) {
    TypeElement top = null;
    for (Element c = e; c != null; c = c.getEnclosingElement()) {
      if (c instanceof TypeElement t) top = t;
      if (c.getKind() == ElementKind.PACKAGE || c.getKind() == ElementKind.MODULE) break;
    }
    return top;
  }

  static TypeElement enclosingType(Element e) {
    for (Element c = e.getEnclosingElement(); c != null; c = c.getEnclosingElement()) {
      if (c instanceof TypeElement t) return t;
    }
    return null;
  }

  boolean unresolved(Element e) {
    if (e instanceof PackageElement p) return p.getEnclosedElements().isEmpty() && !x.packageIds.containsKey(p.getQualifiedName().toString());
    // A field or method that resolved is resolved, whatever its type.
    if (e instanceof VariableElement || e instanceof javax.lang.model.element.ExecutableElement) return false;
    return e.asType().getKind() == TypeKind.ERROR;
  }

  /** Locals, parameters and type parameters: the flow layer's, not a reference's. */
  static boolean local(Element e) {
    return switch (e.getKind()) {
      case PACKAGE, MODULE, LOCAL_VARIABLE, PARAMETER, EXCEPTION_PARAMETER, RESOURCE_VARIABLE, BINDING_VARIABLE, TYPE_PARAMETER, OTHER -> true;
      default -> false;
    };
  }

  /** A field whose value the compiler inlines. */
  static boolean constant(Element e) {
    return e instanceof VariableElement v && v.getKind() == ElementKind.FIELD && v.getConstantValue() != null;
  }
}

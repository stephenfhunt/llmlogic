package codefacts;

import com.sun.source.doctree.BlockTagTree;
import com.sun.source.doctree.DocCommentTree;
import com.sun.source.doctree.DocTree;
import com.sun.source.tree.ClassTree;
import com.sun.source.tree.CompilationUnitTree;
import com.sun.source.tree.LambdaExpressionTree;
import com.sun.source.tree.LineMap;
import com.sun.source.tree.MethodTree;
import com.sun.source.tree.Tree;
import com.sun.source.tree.VariableTree;
import com.sun.source.util.DocSourcePositions;
import com.sun.source.util.DocTrees;
import com.sun.source.util.SourcePositions;
import com.sun.source.util.TreePath;
import com.sun.source.util.TreePathScanner;
import java.util.List;
import java.util.Map;
import java.util.Set;
import javax.lang.model.element.AnnotationMirror;
import javax.lang.model.element.Element;
import javax.lang.model.element.ElementKind;
import javax.lang.model.element.ExecutableElement;
import javax.lang.model.element.Modifier;
import javax.lang.model.element.TypeElement;
import javax.lang.model.element.VariableElement;
import javax.lang.model.type.ArrayType;
import javax.lang.model.type.TypeKind;

/**
 * One file's declarations as javac resolved them, once its source set is
 * analysed. {@link Declarations} gave every declaration its id and a symbol row
 * from its syntax; this pass sets what the compiler knows — modifiers written or
 * implied, visibility, what a variable in an enum is — and writes the rows that
 * need its answer: `param`, `doc` and `jsdoc_tag`, `entry_point` for `main` and
 * test methods.
 */
final class Declared {
  /** The annotations a test framework runs a method for, or calls it around a test. */
  static final Set<String> TEST_ANNOTATIONS = Set.of(
      "org.junit.Test", "org.junit.Before", "org.junit.After", "org.junit.BeforeClass", "org.junit.AfterClass",
      "org.junit.jupiter.api.Test", "org.junit.jupiter.api.RepeatedTest", "org.junit.jupiter.api.TestFactory", "org.junit.jupiter.api.TestTemplate",
      "org.junit.jupiter.api.BeforeEach", "org.junit.jupiter.api.AfterEach", "org.junit.jupiter.api.BeforeAll", "org.junit.jupiter.api.AfterAll",
      "org.junit.jupiter.params.ParameterizedTest",
      "org.testng.annotations.Test", "org.testng.annotations.BeforeMethod", "org.testng.annotations.AfterMethod",
      "org.testng.annotations.BeforeClass", "org.testng.annotations.AfterClass", "org.testng.annotations.BeforeSuite",
      "org.testng.annotations.AfterSuite", "org.testng.annotations.BeforeTest", "org.testng.annotations.AfterTest",
      "org.testng.annotations.DataProvider");
  /** JUnit 5 composes: an annotation carrying `@Testable` makes a test. */
  static final String TESTABLE = "org.junit.platform.commons.annotation.Testable";

  private final Extractor x;
  private final Names n;
  private final Extractor.Source s;
  private final CompilationUnitTree cu;
  private final DocTrees trees;
  private final SourcePositions pos;
  private final LineMap lines;

  Declared(Extractor x, Names n, Extractor.Source s) {
    this.x = x;
    this.n = n;
    this.s = s;
    this.cu = s.cu;
    this.trees = n.trees;
    this.pos = trees.getSourcePositions();
    this.lines = cu.getLineMap();
  }

  void run() {
    TreePath unit = new TreePath(cu);
    String module = x.idByKey.get(s.abs + ":<module>");
    boolean packageInfo = s.abs.getFileName().toString().equals("package-info.java");
    if (module != null) doc(module, packageInfo && cu.getPackage() != null ? new TreePath(unit, cu.getPackage()) : null);
    new TreePathScanner<Void, Void>() {
      @Override
      public Void visitClass(ClassTree t, Void v) {
        declaration(getCurrentPath(), t);
        return super.visitClass(t, v);
      }

      @Override
      public Void visitMethod(MethodTree t, Void v) {
        declaration(getCurrentPath(), t);
        return super.visitMethod(t, v);
      }

      @Override
      public Void visitVariable(VariableTree t, Void v) {
        declaration(getCurrentPath(), t);
        return super.visitVariable(t, v);
      }

      @Override
      public Void visitBlock(com.sun.source.tree.BlockTree t, Void v) {
        // A static initializer the JVM runs when its class initializes.
        TreePath parent = getCurrentPath().getParentPath();
        String id = idOf(t);
        if (t.isStatic() && id != null && parent != null && trees.getElement(parent) instanceof TypeElement owner && !local(owner)) entry(id, "init");
        return super.visitBlock(t, v);
      }

      @Override
      public Void visitLambdaExpression(LambdaExpressionTree t, Void v) {
        String id = idOf(t);
        if (id != null) parameters(id, t.getParameters(), null);
        return super.visitLambdaExpression(t, v);
      }
    }.scan(unit, null);
  }

  private void declaration(TreePath path, Tree t) {
    String id = idOf(t);
    if (id == null) return;
    Map<String, Object> row = x.symbols.get(id);
    Element e = trees.getElement(path);
    if (row == null || e == null || n.unresolved(e)) return;
    Set<Modifier> mods = e.getModifiers();
    boolean local = local(e);
    String kind = (String) row.get("kind");
    if (e.getKind() == ElementKind.ENUM_CONSTANT) row.put("kind", "enum_member");
    // A record component's field is private; the component is what its accessor makes public.
    Element access = e;
    if (e.getKind() == ElementKind.FIELD && e.getEnclosingElement() instanceof TypeElement owner && owner.getKind() == ElementKind.RECORD && !mods.contains(Modifier.STATIC)) {
      row.put("form", "record_component");
      for (javax.lang.model.element.RecordComponentElement rc : owner.getRecordComponents()) {
        if (rc.getSimpleName().equals(e.getSimpleName()) && rc.getAccessor() != null) access = rc.getAccessor();
      }
    }
    boolean member = kind.equals("class") || kind.equals("interface") || kind.equals("method") || kind.equals("constructor") || kind.equals("property");
    if (member) {
      row.put("visibility", local && e instanceof TypeElement ? null : visibility(access));
      row.put("exported", !local && exported(access));
      row.put("is_static", mods.contains(Modifier.STATIC));
      // An interface is abstract to javac; the schema's `abstract` is a class or member written or implied so.
      row.put("is_abstract", mods.contains(Modifier.ABSTRACT) && !(e instanceof TypeElement te && te.getKind().isInterface()));
    }
    if (e instanceof VariableElement) row.put("is_readonly", mods.contains(Modifier.FINAL));

    if (!local && (e instanceof TypeElement || e instanceof ExecutableElement || e.getKind().isField())) {
      boolean javadocDeprecated = doc(id, path);
      if (!javadocDeprecated && annotated(e, "java.lang.Deprecated")) x.em.emit("jsdoc_tag", Main.row("symbol", id, "tag", "deprecated", "text", null));
    }
    if (e instanceof ExecutableElement m && t instanceof MethodTree mt) {
      parameters(id, mt.getParameters(), m);
      if (main(m)) entry(id, "main");
      if (test(m)) entry(id, "test");
    }
  }

  private void parameters(String fn, List<? extends VariableTree> params, ExecutableElement m) {
    for (int i = 0; i < params.size(); i++) {
      VariableTree p = params.get(i);
      String id = idOf(p);
      if (id == null) continue;
      boolean rest = m != null && m.isVarArgs() && i == params.size() - 1;
      x.em.emit("param", Main.row("fn", fn, "index", i, "symbol", id, "name", p.getName().toString(), "optional", false, "rest", rest, "has_default", false));
    }
  }

  /** The doc row and each block tag, as javac reads the comment; whether it says `@deprecated`. */
  private boolean doc(String id, TreePath path) {
    DocCommentTree dc = path == null ? null : trees.getDocCommentTree(path);
    int[] span = dc == null ? null : Text.docBefore(s.text, (int) pos.getStartPosition(cu, path.getLeaf()));
    if (dc == null || span == null) {
      x.em.emit("doc", Main.row("symbol", id, "has_doc", false, "lines", 0));
      return false;
    }
    x.em.emit("doc", Main.row("symbol", id, "has_doc", true, "lines", lines.getLineNumber(span[1] - 1) - lines.getLineNumber(span[0]) + 1));
    DocSourcePositions dpos = trees.getSourcePositions();
    boolean deprecated = false;
    for (DocTree tag : dc.getBlockTags()) {
      if (!(tag instanceof BlockTagTree b)) continue;
      String name = b.getTagName();
      long a = dpos.getStartPosition(cu, dc, tag);
      long z = dpos.getEndPosition(cu, dc, tag);
      String text = null;
      if (a >= 0 && z > a) {
        String raw = s.text.substring((int) a, (int) z).replaceFirst("^@" + java.util.regex.Pattern.quote(name), "");
        String joined = String.join(" ", raw.lines().map(l -> l.strip().replaceFirst("^\\*\\s?", "").strip()).filter(l -> !l.isEmpty()).toList());
        text = joined.isEmpty() ? null : Text.truncate(joined, 200);
      }
      x.em.emit("jsdoc_tag", Main.row("symbol", id, "tag", name, "text", text));
      if (name.equals("deprecated")) deprecated = true;
    }
    return deprecated;
  }

  private void entry(String id, String kind) {
    x.em.emit("entry_point", Main.row("symbol", id, "kind", kind));
  }

  /**
   * A main method the launcher accepts: `public static void main(String[])`, and
   * from Java 25 any non-private `void main()` or `void main(String[])`.
   */
  private boolean main(ExecutableElement m) {
    if (!m.getSimpleName().contentEquals("main") || m.getReturnType().getKind() != TypeKind.VOID) return false;
    List<? extends VariableElement> params = m.getParameters();
    boolean strings = params.size() == 1 && params.get(0).asType() instanceof ArrayType a
        && n.types.isSameType(a.getComponentType(), n.elements.getTypeElement("java.lang.String").asType());
    Set<Modifier> mods = m.getModifiers();
    if (strings && mods.contains(Modifier.PUBLIC) && mods.contains(Modifier.STATIC)) return true;
    Integer release = s.unit.release;
    boolean instanceMains = release == null ? Runtime.version().feature() >= 25 : release >= 25;
    return instanceMains && !mods.contains(Modifier.PRIVATE) && (params.isEmpty() || strings);
  }

  /**
   * A method a test framework runs: an annotation of JUnit's or TestNG's, or one
   * carrying `@Testable`. An annotation whose type did not resolve (no test jar on
   * the classpath) is judged by its simple name.
   */
  private boolean test(ExecutableElement m) {
    for (AnnotationMirror a : m.getAnnotationMirrors()) {
      TypeElement type = (TypeElement) a.getAnnotationType().asElement();
      if (a.getAnnotationType().getKind() == TypeKind.ERROR) {
        String simple = type.getSimpleName().toString();
        if (TEST_ANNOTATIONS.stream().anyMatch(q -> q.endsWith("." + simple))) return true;
        continue;
      }
      if (TEST_ANNOTATIONS.contains(type.getQualifiedName().toString()) || annotated(type, TESTABLE)) return true;
    }
    return false;
  }

  private static boolean annotated(Element e, String qualified) {
    for (AnnotationMirror a : e.getAnnotationMirrors()) {
      if (a.getAnnotationType().asElement() instanceof TypeElement t && t.getQualifiedName().contentEquals(qualified)) return true;
    }
    return false;
  }

  /** Below local scope: a local or anonymous class, what it declares, and anything inside a method or initializer. */
  private static boolean local(Element e) {
    for (Element c = e; c != null; c = c.getEnclosingElement()) {
      if (c != e) {
        switch (c.getKind()) {
          case METHOD, CONSTRUCTOR, STATIC_INIT, INSTANCE_INIT -> {
            return true;
          }
          default -> {}
        }
      }
      if (c instanceof TypeElement t && (t.getNestingKind() == javax.lang.model.element.NestingKind.LOCAL || t.getNestingKind() == javax.lang.model.element.NestingKind.ANONYMOUS)) return true;
      if (c.getKind() == ElementKind.PACKAGE || c.getKind() == ElementKind.MODULE) return false;
    }
    return false;
  }

  private static String visibility(Element e) {
    Set<Modifier> mods = e.getModifiers();
    if (mods.contains(Modifier.PUBLIC)) return "public";
    if (mods.contains(Modifier.PROTECTED)) return "protected";
    if (mods.contains(Modifier.PRIVATE)) return "private";
    return "package";
  }

  /** Reachable from outside its package: public or protected, in public or protected types all the way out. */
  private static boolean exported(Element e) {
    for (Element c = e; c != null && !(c.getKind() == ElementKind.PACKAGE); c = c.getEnclosingElement()) {
      Set<Modifier> mods = c.getModifiers();
      boolean top = c.getEnclosingElement() != null && c.getEnclosingElement().getKind() == ElementKind.PACKAGE;
      if (top ? !mods.contains(Modifier.PUBLIC) : !(mods.contains(Modifier.PUBLIC) || mods.contains(Modifier.PROTECTED))) return false;
    }
    return true;
  }

  private String idOf(Tree t) {
    return x.idByKey.get(Extractor.key(s.abs, pos.getStartPosition(cu, t), t));
  }
}

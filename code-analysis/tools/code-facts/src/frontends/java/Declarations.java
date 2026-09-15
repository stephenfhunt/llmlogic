package codefacts;

import com.sun.source.tree.AnnotationTree;
import com.sun.source.tree.BlockTree;
import com.sun.source.tree.ClassTree;
import com.sun.source.tree.CompilationUnitTree;
import com.sun.source.tree.LambdaExpressionTree;
import com.sun.source.tree.LineMap;
import com.sun.source.tree.MethodTree;
import com.sun.source.tree.ModifiersTree;
import com.sun.source.tree.NewClassTree;
import com.sun.source.tree.PrimitiveTypeTree;
import com.sun.source.tree.Tree;
import com.sun.source.tree.TypeParameterTree;
import com.sun.source.tree.VariableTree;
import com.sun.source.util.SourcePositions;
import com.sun.source.util.TreeScanner;
import com.sun.source.util.Trees;
import java.util.List;
import java.util.Set;
import javax.lang.model.element.Modifier;
import javax.lang.model.type.TypeKind;
import javax.tools.Diagnostic;

/**
 * One file's declarations, from its syntax: an id for each, and the rows that
 * describe it — `symbol`, `param`, `doc`, `jsdoc_tag`, `entry_point`, `exports`.
 * What only a resolved name can tell (annotations' types, imports) waits for
 * {@link References}.
 */
final class Declarations {
  /** The methods a test framework runs or calls around a test, by annotation name. */
  static final Set<String> TEST_ANNOTATIONS = Set.of(
      "Test", "ParameterizedTest", "RepeatedTest", "TestFactory", "TestTemplate",
      "BeforeEach", "AfterEach", "BeforeAll", "AfterAll", "Before", "After", "BeforeClass", "AfterClass",
      "BeforeMethod", "AfterMethod", "BeforeSuite", "AfterSuite", "BeforeTest", "AfterTest", "DataProvider");

  private final Extractor x;
  private final Extractor.Source s;
  private final CompilationUnitTree cu;
  private final SourcePositions pos;
  private final LineMap lines;
  private final String text;

  /** What encloses a declaration: a type's kind when it is a member, and whether it is below local scope. */
  private record Owner(String id, Tree.Kind kind, boolean exported, boolean local) {}

  Declarations(Extractor x, Extractor.Source s) {
    this.x = x;
    this.s = s;
    this.cu = s.cu;
    this.pos = Trees.instance(s.unit.task).getSourcePositions();
    this.lines = cu.getLineMap();
    this.text = s.text;
  }

  void run() {
    long loc = Text.loc(text);
    String module = x.claim(s.path + "#<module>", s.abs + ":<module>", 1, 1);
    x.addSymbol(module, "<module>", "module", null, s, 1, Math.max(loc, 1), null, false, null, false, false, false);
    boolean packageInfo = s.abs.getFileName().toString().equals("package-info.java");
    doc(module, packageInfo && cu.getPackage() != null ? start(cu.getPackage()) : -1);
    if (!s.pkg.isEmpty() && !x.packageIds.containsKey(s.pkg)) {
      String id = x.claim(Extractor.dirOf(s.path) + "#<package>", "package:" + s.pkg, 1, 1);
      x.addSymbol(id, s.pkg, "namespace", null, s, 1, 1, null, false, null, false, false, false);
      x.packageIds.put(s.pkg, id);
      x.packageDirs.put(s.pkg, Extractor.dirOf(s.path));
    }
    Owner top = new Owner(module, null, true, false);
    for (Tree t : cu.getTypeDecls()) {
      if (t instanceof ClassTree c) type(c, top);
    }
  }

  // ── declarations ───────────────────────────────────────────────────────────

  private void type(ClassTree c, Owner owner) {
    long start = start(c);
    boolean anonymous = c.getSimpleName().length() == 0;
    boolean top = owner.kind() == null && !owner.local();
    String segment = anonymous ? "<class@" + line(start) + ":" + col(start) + ">" : c.getSimpleName().toString();
    String id = x.claim(Extractor.member(owner.id(), segment), key(c), line(start), col(start));
    Set<Modifier> mods = c.getModifiers().getFlags();
    String kind = "class";
    String form = null;
    switch (c.getKind()) {
      case INTERFACE -> kind = "interface";
      case ANNOTATION_TYPE -> {
        kind = "interface";
        form = "annotation";
      }
      case ENUM -> form = "enum";
      case RECORD -> form = "record";
      default -> {}
    }
    String visibility = owner.local() ? null : visibility(mods, owner.kind());
    boolean exported = !owner.local() && (top ? mods.contains(Modifier.PUBLIC) : owner.exported() && api(visibility));
    // A nested enum, record, interface or annotation type is static, as is any member type of an interface.
    boolean isStatic = mods.contains(Modifier.STATIC) || (!top && (c.getKind() != Tree.Kind.CLASS || iface(owner.kind())));
    x.addSymbol(id, anonymous ? "<class>" : segment, kind, form, s, declLine(c, c.getModifiers()), line(end(c)), owner.id(),
        exported, visibility, isStatic, mods.contains(Modifier.ABSTRACT), false);
    boolean deprecated = !owner.local() && doc(id, start);
    annotations(id, c.getModifiers(), deprecated, false);
    if (top && mods.contains(Modifier.PUBLIC)) {
      x.em.emit("exports", Main.row("file", s.path, "name", segment, "symbol", id, "kind", "local", "is_type", true));
    }
    typeParameters(c.getTypeParameters(), id);
    Owner inner = new Owner(id, c.getKind(), exported, owner.local());
    for (Tree m : c.getMembers()) {
      if (m instanceof ClassTree nested) type(nested, inner);
      else if (m instanceof MethodTree method) method(method, inner);
      else if (m instanceof VariableTree field) field(field, inner);
      else if (m instanceof BlockTree block) initializer(block, inner);
    }
  }

  private void method(MethodTree m, Owner owner) {
    long start = start(m);
    boolean ctor = m.getName().contentEquals("<init>");
    String segment = ctor ? "constructor" : m.getName().toString();
    String id = x.claim(Extractor.member(owner.id(), segment), key(m), line(start), col(start));
    Set<Modifier> mods = m.getModifiers().getFlags();
    String visibility = visibility(mods, owner.kind());
    boolean isAbstract = mods.contains(Modifier.ABSTRACT)
        || (iface(owner.kind()) && m.getBody() == null && !mods.contains(Modifier.STATIC) && !mods.contains(Modifier.DEFAULT) && !mods.contains(Modifier.PRIVATE));
    x.addSymbol(id, segment, ctor ? "constructor" : "method", null, s, declLine(m, m.getModifiers()), line(end(m)), owner.id(),
        owner.exported() && !owner.local() && api(visibility), visibility, mods.contains(Modifier.STATIC), isAbstract, false);
    boolean deprecated = !owner.local() && doc(id, start);
    annotations(id, m.getModifiers(), deprecated, true);
    typeParameters(m.getTypeParameters(), id);
    parameters(id, m.getParameters());
    if (!ctor && isMain(m, mods)) entry(id, "main");
    Owner body = new Owner(id, null, false, true);
    if (m.getBody() != null) new Body().scan(m.getBody(), body);
    if (m.getDefaultValue() != null) new Body().scan(m.getDefaultValue(), body);
  }

  private void field(VariableTree v, Owner owner) {
    long start = start(v);
    Set<Modifier> mods = v.getModifiers().getFlags();
    boolean constant = owner.kind() == Tree.Kind.ENUM && enumConstant(v);
    boolean component = owner.kind() == Tree.Kind.RECORD && !mods.contains(Modifier.STATIC);
    String id = x.claim(Extractor.member(owner.id(), v.getName().toString()), key(v), line(start), col(start));
    // An enum constant is public; a record component is read through its public accessor.
    String visibility = constant || component ? "public" : visibility(mods, owner.kind());
    x.addSymbol(id, v.getName().toString(), constant ? "enum_member" : "property", component ? "record_component" : null, s,
        constant ? line(start) : declLine(v, v.getModifiers()), line(end(v)), owner.id(),
        owner.exported() && !owner.local() && api(visibility), visibility,
        constant || mods.contains(Modifier.STATIC) || iface(owner.kind()), false,
        constant || component || iface(owner.kind()) || mods.contains(Modifier.FINAL));
    boolean deprecated = !owner.local() && doc(id, start);
    annotations(id, v.getModifiers(), deprecated, false);
    if (v.getInitializer() != null) new Body().scan(v.getInitializer(), new Owner(id, null, false, true));
  }

  private void initializer(BlockTree b, Owner owner) {
    long start = start(b);
    String name = b.isStatic() ? "<static>" : "<instance>";
    String id = x.claim(Extractor.member(owner.id(), name.substring(0, name.length() - 1) + "@" + line(start) + ":" + col(start) + ">"), key(b), line(start), col(start));
    x.addSymbol(id, name, "static_block", b.isStatic() ? "static_init" : "instance_init", s, line(start), line(end(b)), owner.id(),
        false, null, b.isStatic(), false, false);
    if (b.isStatic() && !owner.local()) entry(id, "init");
    new Body().scan(b, new Owner(id, null, false, true));
  }

  private void parameters(String fn, List<? extends VariableTree> params) {
    int index = 0;
    for (VariableTree p : params) {
      long start = start(p);
      String name = p.getName().toString();
      String id = x.claim(Extractor.member(fn, name), key(p), line(start), col(start));
      x.addSymbol(id, name, "parameter", null, s, line(start), line(end(p)), fn, false, null, false, false, p.getModifiers().getFlags().contains(Modifier.FINAL));
      annotations(id, p.getModifiers(), false, false);
      x.em.emit("param", Main.row("fn", fn, "index", index, "symbol", id, "name", name, "optional", false, "rest", varargs(p), "has_default", false));
      index++;
    }
  }

  private void typeParameters(List<? extends TypeParameterTree> params, String parent) {
    for (TypeParameterTree tp : params) {
      long start = start(tp);
      String name = tp.getName().toString();
      String id = x.claim(Extractor.member(parent, name), key(tp), line(start), col(start));
      x.addSymbol(id, name, "type_parameter", null, s, line(start), line(end(tp)), parent, false, null, false, false, false);
    }
  }

  /** Locals, lambdas, and local and anonymous classes, below a member. */
  private final class Body extends TreeScanner<Void, Owner> {
    @Override
    public Void visitVariable(VariableTree v, Owner owner) {
      long start = start(v);
      String name = v.getName().toString();
      String id = x.claim(Extractor.member(owner.id(), name), key(v), line(start), col(start));
      x.addSymbol(id, name, "local", null, s, line(start), line(end(v)), owner.id(), false, null, false, false, v.getModifiers().getFlags().contains(Modifier.FINAL));
      annotations(id, v.getModifiers(), false, false);
      return scan(v.getInitializer(), owner);
    }

    @Override
    public Void visitClass(ClassTree c, Owner owner) {
      type(c, new Owner(owner.id(), null, false, true));
      return null;
    }

    @Override
    public Void visitNewClass(NewClassTree n, Owner owner) {
      scan(n.getEnclosingExpression(), owner);
      scan(n.getArguments(), owner);
      if (n.getClassBody() != null) type(n.getClassBody(), new Owner(owner.id(), null, false, true));
      return null;
    }

    @Override
    public Void visitLambdaExpression(LambdaExpressionTree l, Owner owner) {
      long start = start(l);
      String id = x.claim(Extractor.member(owner.id(), "<lambda@" + line(start) + ":" + col(start) + ">"), key(l), line(start), col(start));
      x.addSymbol(id, "<lambda>", "function", null, s, line(start), line(end(l)), owner.id(), false, null, false, false, false);
      parameters(id, l.getParameters());
      return scan(l.getBody(), new Owner(id, null, false, true));
    }
  }

  // ── what a declaration says about itself ───────────────────────────────────

  /** The doc row, and each block tag; whether it carries `@deprecated`. A negative start means no comment can apply. */
  private boolean doc(String id, long start) {
    int[] span = start < 0 ? null : Text.docBefore(text, (int) start);
    if (span == null) {
      x.em.emit("doc", Main.row("symbol", id, "has_doc", false, "lines", 0));
      return false;
    }
    x.em.emit("doc", Main.row("symbol", id, "has_doc", true, "lines", line(span[1] - 1) - line(span[0]) + 1));
    boolean deprecated = false;
    for (Text.Tag t : Text.blockTags(text.substring(span[0], span[1]))) {
      x.em.emit("jsdoc_tag", Main.row("symbol", id, "tag", t.name(), "text", t.text()));
      if (t.name().equals("deprecated")) deprecated = true;
    }
    return deprecated;
  }

  /** `@Deprecated` as the deprecated tag when the Javadoc has none; a test framework's annotations as a test entry point. */
  private void annotations(String id, ModifiersTree mods, boolean docDeprecated, boolean method) {
    boolean tagged = docDeprecated;
    boolean test = false;
    for (AnnotationTree a : mods.getAnnotations()) {
      String written = source(a.getAnnotationType());
      String simple = written.substring(written.lastIndexOf('.') + 1);
      if (!tagged && simple.equals("Deprecated")) {
        x.em.emit("jsdoc_tag", Main.row("symbol", id, "tag", "deprecated", "text", null));
        tagged = true;
      }
      if (method && !test && TEST_ANNOTATIONS.contains(simple)) {
        entry(id, "test");
        test = true;
      }
    }
  }

  private void entry(String id, String kind) {
    x.em.emit("entry_point", Main.row("symbol", id, "kind", kind));
  }

  /**
   * A main method the launcher accepts: `public static void main(String[])`,
   * and from Java 25 any non-private `void main()` or `void main(String[])`.
   */
  private boolean isMain(MethodTree m, Set<Modifier> mods) {
    if (!m.getName().contentEquals("main")) return false;
    if (!(m.getReturnType() instanceof PrimitiveTypeTree p) || p.getPrimitiveTypeKind() != TypeKind.VOID) return false;
    List<? extends VariableTree> params = m.getParameters();
    boolean strings = params.size() == 1 && stringArray(params.get(0));
    if (strings && mods.contains(Modifier.PUBLIC) && mods.contains(Modifier.STATIC)) return true;
    Integer release = s.unit.release;
    boolean instanceMains = release == null ? Runtime.version().feature() >= 25 : release >= 25;
    return instanceMains && !mods.contains(Modifier.PRIVATE) && (params.isEmpty() || strings);
  }

  private boolean stringArray(VariableTree p) {
    String t = source(p.getType()).replaceAll("\\s+", "");
    String rest = text.substring((int) end(p.getType()), (int) end(p)).replaceAll("\\s+", "");
    if (rest.startsWith("...")) t = t + "...";
    return t.equals("String[]") || t.equals("java.lang.String[]") || t.equals("String...") || t.equals("java.lang.String...");
  }

  private boolean varargs(VariableTree p) {
    long typeEnd = end(p.getType());
    if (typeEnd < 0) return false;
    String between = text.substring((int) typeEnd, (int) end(p));
    return source(p.getType()).endsWith("...") || between.stripLeading().startsWith("...");
  }

  /** An enum constant's type is the enum, written nowhere. */
  private boolean enumConstant(VariableTree v) {
    return end(v.getType()) == Diagnostic.NOPOS;
  }

  // ── positions ──────────────────────────────────────────────────────────────

  private static boolean iface(Tree.Kind k) {
    return k == Tree.Kind.INTERFACE || k == Tree.Kind.ANNOTATION_TYPE;
  }

  private static boolean api(String visibility) {
    return "public".equals(visibility) || "protected".equals(visibility);
  }

  /** The modifier written, or what the member's type implies: public in an interface, package otherwise. */
  private static String visibility(Set<Modifier> mods, Tree.Kind owner) {
    if (mods.contains(Modifier.PUBLIC)) return "public";
    if (mods.contains(Modifier.PROTECTED)) return "protected";
    if (mods.contains(Modifier.PRIVATE)) return "private";
    return iface(owner) ? "public" : "package";
  }

  String key(Tree t) {
    return s.abs + ":" + start(t);
  }

  private long start(Tree t) {
    return pos.getStartPosition(cu, t);
  }

  private long end(Tree t) {
    return pos.getEndPosition(cu, t);
  }

  private long line(long offset) {
    return lines.getLineNumber(Math.max(offset, 0));
  }

  private long col(long offset) {
    return lines.getColumnNumber(Math.max(offset, 0));
  }

  /** The line a declaration's own tokens begin on, past its annotations and modifiers. */
  private long declLine(Tree t, ModifiersTree mods) {
    long start = start(t);
    long afterMods = end(mods);
    long from = afterMods > start ? afterMods : start;
    return line(Text.nextToken(text, (int) from));
  }

  private String source(Tree t) {
    long a = start(t);
    long b = end(t);
    return a < 0 || b < a ? t.toString() : text.substring((int) a, (int) b);
  }
}

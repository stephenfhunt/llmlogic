package codefacts;

import com.sun.source.tree.BlockTree;
import com.sun.source.tree.ClassTree;
import com.sun.source.tree.DirectiveTree;
import com.sun.source.tree.ExportsTree;
import com.sun.source.tree.ExpressionTree;
import com.sun.source.tree.CompilationUnitTree;
import com.sun.source.tree.LambdaExpressionTree;
import com.sun.source.tree.LineMap;
import com.sun.source.tree.MethodTree;
import com.sun.source.tree.ModuleTree;
import com.sun.source.tree.OpensTree;
import com.sun.source.tree.ProvidesTree;
import com.sun.source.tree.RequiresTree;
import com.sun.source.tree.UsesTree;
import com.sun.source.tree.ModifiersTree;
import com.sun.source.tree.NewClassTree;
import com.sun.source.tree.Tree;
import com.sun.source.tree.TypeParameterTree;
import com.sun.source.tree.VariableTree;
import com.sun.source.util.SourcePositions;
import com.sun.source.util.TreeScanner;
import com.sun.source.util.Trees;
import java.util.ArrayList;
import java.util.List;
import java.util.Set;
import javax.lang.model.element.Modifier;

/**
 * One file's declarations, from its syntax: an id for each, assigned before any
 * source set is analysed, a symbol row with the modifiers as written, and
 * `exports`. What the compiler knows — implied modifiers, what a variable in an
 * enum is, varargs, Javadoc, entry points — {@link Declared} sets once javac has
 * analysed the file.
 */
final class Declarations {
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
    if (!s.pkg.isEmpty() && !x.packageIds.containsKey(s.pkg)) {
      String id = x.claim(Extractor.dirOf(s.path) + "#<package>", "package:" + s.pkg, 1, 1);
      x.addSymbol(id, s.pkg, "namespace", null, s, 1, 1, null, false, null, false, false, false);
      x.packageIds.put(s.pkg, id);
      x.packageDirs.put(s.pkg, Extractor.dirOf(s.path));
    }
    moduleDirectives();
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
    String visibility = owner.local() ? null : written(mods);
    boolean exported = !owner.local() && (top ? mods.contains(Modifier.PUBLIC) : owner.exported() && api(visibility));
    x.addSymbol(id, anonymous ? "<class>" : segment, kind, form, s, declLine(c, c.getModifiers()), line(end(c)), owner.id(),
        exported, visibility, mods.contains(Modifier.STATIC), mods.contains(Modifier.ABSTRACT), false);
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
    String visibility = written(mods);
    x.addSymbol(id, segment, ctor ? "constructor" : "method", null, s, declLine(m, m.getModifiers()), line(end(m)), owner.id(),
        owner.exported() && !owner.local() && api(visibility), visibility, mods.contains(Modifier.STATIC), mods.contains(Modifier.ABSTRACT), false);
    typeParameters(m.getTypeParameters(), id);
    parameters(id, m.getParameters());
    Owner body = new Owner(id, null, false, true);
    if (m.getBody() != null) new Body().scan(m.getBody(), body);
    if (m.getDefaultValue() != null) new Body().scan(m.getDefaultValue(), body);
  }

  private void field(VariableTree v, Owner owner) {
    long start = start(v);
    Set<Modifier> mods = v.getModifiers().getFlags();
    String id = x.claim(Extractor.member(owner.id(), v.getName().toString()), key(v), line(start), col(start));
    String visibility = written(mods);
    x.addSymbol(id, v.getName().toString(), "property", null, s, declLine(v, v.getModifiers()), line(end(v)), owner.id(),
        owner.exported() && !owner.local() && api(visibility), visibility, mods.contains(Modifier.STATIC), false, mods.contains(Modifier.FINAL));
    if (v.getInitializer() != null) new Body().scan(v.getInitializer(), new Owner(id, null, false, true));
  }

  private void initializer(BlockTree b, Owner owner) {
    long start = start(b);
    String name = b.isStatic() ? "<static>" : "<instance>";
    String id = x.claim(Extractor.member(owner.id(), name.substring(0, name.length() - 1) + "@" + line(start) + ":" + col(start) + ">"), key(b), line(start), col(start));
    x.addSymbol(id, name, "static_block", b.isStatic() ? "static_init" : "instance_init", s, line(start), line(end(b)), owner.id(),
        false, null, b.isStatic(), false, false);
    new Body().scan(b, new Owner(id, null, false, true));
  }

  private void parameters(String fn, List<? extends VariableTree> params) {
    for (VariableTree p : params) {
      long start = start(p);
      String name = p.getName().toString();
      String id = x.claim(Extractor.member(fn, name), key(p), line(start), col(start));
      x.addSymbol(id, name, "parameter", null, s, line(start), line(end(p)), fn, false, null, false, false, p.getModifiers().getFlags().contains(Modifier.FINAL));
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

  /**
   * What a `module-info.java` declares: the module itself, and a row per
   * directive — a row per target where one names several. Names are as written;
   * the types a `uses` or `provides` names resolve as `ref` rows too, which is
   * where a service implementation nothing calls becomes visible.
   */
  private void moduleDirectives() {
    ModuleTree m = cu.getModule();
    if (m == null) return;
    String pkg = s.unit.module.name();
    emit(pkg, "module", null, m.getName().toString(), null, m.getModuleType() == ModuleTree.ModuleKind.OPEN ? "open" : null);
    String declared = m.getName().toString();
    for (DirectiveTree d : m.getDirectives()) {
      if (d instanceof RequiresTree r) {
        List<String> mods = new ArrayList<>();
        if (r.isTransitive()) mods.add("transitive");
        if (r.isStatic()) mods.add("static");
        emit(pkg, "requires", r.getModuleName().toString(), declared, null, mods.isEmpty() ? null : String.join(" ", mods));
      } else if (d instanceof ExportsTree e) {
        targets(pkg, "exports", e.getPackageName().toString(), declared, e.getModuleNames());
      } else if (d instanceof OpensTree o) {
        targets(pkg, "opens", o.getPackageName().toString(), declared, o.getModuleNames());
      } else if (d instanceof UsesTree u) {
        emit(pkg, "uses", u.getServiceName().toString(), declared, null, null);
      } else if (d instanceof ProvidesTree pr) {
        for (ExpressionTree impl : pr.getImplementationNames()) {
          emit(pkg, "provides", pr.getServiceName().toString(), declared, impl.toString(), null);
        }
      }
    }
  }

  /** A qualified `exports` or `opens` is a row per module it names; an unqualified one, a row. */
  private void targets(String pkg, String directive, String path, String declared, List<? extends ExpressionTree> to) {
    if (to == null || to.isEmpty()) {
      emit(pkg, directive, path, declared, null, null);
      return;
    }
    for (ExpressionTree t : to) emit(pkg, directive, path, declared, t.toString(), null);
  }

  private void emit(String pkg, String directive, String path, String module, String target, String modifier) {
    x.em.emit("module_directive", Main.row(
        "package", pkg, "directive", directive, "path", path, "version", null, "replacement", null,
        "replacement_version", null, "module", module, "target", target, "modifier", modifier));
  }

  /** Locals, lambdas, and local and anonymous classes, below a member. */
  private final class Body extends TreeScanner<Void, Owner> {
    @Override
    public Void visitVariable(VariableTree v, Owner owner) {
      long start = start(v);
      String name = v.getName().toString();
      String id = x.claim(Extractor.member(owner.id(), name), key(v), line(start), col(start));
      x.addSymbol(id, name, "local", null, s, line(start), line(end(v)), owner.id(), false, null, false, false, v.getModifiers().getFlags().contains(Modifier.FINAL));
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

  // ── positions ──────────────────────────────────────────────────────────────

  private static boolean api(String visibility) {
    return "public".equals(visibility) || "protected".equals(visibility);
  }

  /** The access modifier written; what an interface or enum implies, javac's elements say ({@link Declared}). */
  private static String written(Set<Modifier> mods) {
    if (mods.contains(Modifier.PUBLIC)) return "public";
    if (mods.contains(Modifier.PROTECTED)) return "protected";
    if (mods.contains(Modifier.PRIVATE)) return "private";
    return "package";
  }

  String key(Tree t) {
    return Extractor.key(s.abs, start(t), t);
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

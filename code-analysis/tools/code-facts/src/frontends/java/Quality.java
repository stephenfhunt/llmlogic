package codefacts;

import com.sun.source.tree.AnnotatedTypeTree;
import com.sun.source.tree.AnnotationTree;
import com.sun.source.tree.ArrayTypeTree;
import com.sun.source.tree.AssignmentTree;
import com.sun.source.tree.CatchTree;
import com.sun.source.tree.ClassTree;
import com.sun.source.tree.CompilationUnitTree;
import com.sun.source.tree.ExpressionStatementTree;
import com.sun.source.tree.ExpressionTree;
import com.sun.source.tree.IdentifierTree;
import com.sun.source.tree.ImportTree;
import com.sun.source.tree.InstanceOfTree;
import com.sun.source.tree.LambdaExpressionTree;
import com.sun.source.tree.LineMap;
import com.sun.source.tree.LiteralTree;
import com.sun.source.tree.MemberReferenceTree;
import com.sun.source.tree.MemberSelectTree;
import com.sun.source.tree.MethodInvocationTree;
import com.sun.source.tree.MethodTree;
import com.sun.source.tree.NewArrayTree;
import com.sun.source.tree.NewClassTree;
import com.sun.source.tree.PackageTree;
import com.sun.source.tree.ParameterizedTypeTree;
import com.sun.source.tree.ParenthesizedTree;
import com.sun.source.tree.ThrowTree;
import com.sun.source.tree.Tree;
import com.sun.source.tree.TypeCastTree;
import com.sun.source.util.DocTrees;
import com.sun.source.util.SourcePositions;
import com.sun.source.util.TreePath;
import com.sun.source.util.TreePathScanner;
import com.sun.source.util.TreeScanner;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.regex.Matcher;
import java.util.regex.Pattern;
import javax.lang.model.element.Element;
import javax.lang.model.element.ExecutableElement;
import javax.lang.model.element.TypeElement;
import javax.lang.model.element.VariableElement;
import javax.lang.model.type.DeclaredType;
import javax.lang.model.type.TypeKind;
import javax.lang.model.type.TypeMirror;
import javax.lang.model.util.Elements;

/**
 * The quality layer for one file, once its source set is analysed: the places
 * its authors suppressed, deferred or overrode something — suppressions written
 * as annotations and as comments, markers, casts, raw types, literals, throws and
 * catches, and futures nothing waits on. The source set's diagnostics are
 * {@link Extractor}'s.
 */
final class Quality {
  private static final Pattern MARKER = Pattern.compile("\\b(TODO|FIXME|HACK|XXX)\\b[:\\s-]*(.*)");
  private static final Pattern SONAR = Pattern.compile("\\bNOSONAR\\b");
  private static final Pattern PMD = Pattern.compile("\\bNOPMD\\b");
  private static final Pattern CHECKSTYLE = Pattern.compile("\\bCHECKSTYLE[:.](OFF|ON|SUPPRESS)\\b\\s*:?\\s*(.*)");
  private static final Pattern CHECKSTYLE_NEARBY = Pattern.compile("\\bSUPPRESS CHECKSTYLE\\b\\s*(.*)");
  private static final Pattern SPOTLESS = Pattern.compile("\\bspotless:(off|on)\\b");

  private final Extractor x;
  private final Names n;
  private final Extractor.Source s;
  private final CompilationUnitTree cu;
  private final DocTrees trees;
  private final SourcePositions pos;
  private final LineMap lines;

  Quality(Extractor x, Names n, Extractor.Source s) {
    this.x = x;
    this.n = n;
    this.s = s;
    this.cu = s.cu;
    this.trees = n.trees;
    this.pos = trees.getSourcePositions();
    this.lines = cu.getLineMap();
  }

  void run() {
    comments();
    new Scanner().scan(new TreePath(cu), null);
  }

  // ── comments ───────────────────────────────────────────────────────────────

  /** Suppressions and markers, a line of each comment at a time. */
  private void comments() {
    for (int[] r : Text.comments(s.text)) {
      String[] parts = s.text.substring(r[0], r[1]).split("\n", -1);
      long first = lines.getLineNumber(r[0]);
      for (int i = 0; i < parts.length; i++) {
        String body = parts[i].replaceFirst("\\*/\\s*$", "");
        long line = first + i;
        if (SONAR.matcher(body).find()) lint(line, "sonar", "nosonar", null);
        if (PMD.matcher(body).find()) lint(line, "pmd", "nopmd", null);
        Matcher m = CHECKSTYLE.matcher(body);
        if (m.find()) lint(line, "checkstyle", m.group(1).toLowerCase(java.util.Locale.ROOT), m.group(2));
        m = CHECKSTYLE_NEARBY.matcher(body);
        if (m.find()) lint(line, "checkstyle", "suppress", m.group(1));
        m = SPOTLESS.matcher(body);
        if (m.find()) lint(line, "spotless", m.group(1), null);
        m = MARKER.matcher(body);
        if (m.find()) {
          x.em.emit("comment_marker", Main.row("file", s.path, "line", line, "kind", m.group(1).toLowerCase(java.util.Locale.ROOT), "text", Text.truncate(m.group(2).stripTrailing(), 200)));
        }
      }
    }
  }

  private void lint(long line, String tool, String directive, String rules) {
    String r = rules == null || rules.isBlank() ? null : Text.truncate(rules.strip(), 200);
    x.em.emit("lint_directive", Main.row("file", s.path, "line", line, "tool", tool, "directive", directive, "rules", r));
  }

  // ── trees ──────────────────────────────────────────────────────────────────

  private final class Scanner extends TreePathScanner<Void, Void> {
    @Override
    public Void visitImport(ImportTree t, Void v) {
      return null;
    }

    @Override
    public Void visitPackage(PackageTree t, Void v) {
      return null;
    }

    @Override
    public Void visitMethod(MethodTree t, Void v) {
      // What the compiler writes — a default constructor, a record's accessors — is no one's.
      if (trees.getElement(getCurrentPath()) instanceof ExecutableElement m && n.elements.getOrigin(m) != Elements.Origin.EXPLICIT) return null;
      return super.visitMethod(t, v);
    }

    /** A suppression; its arguments are no literals of the code's (`decorator.text` has them). */
    @Override
    public Void visitAnnotation(AnnotationTree t, Void v) {
      suppression(getCurrentPath(), t);
      return null;
    }

    @Override
    public Void visitLiteral(LiteralTree t, Void v) {
      literal(getCurrentPath(), t);
      return null;
    }

    @Override
    public Void visitIdentifier(IdentifierTree t, Void v) {
      raw(getCurrentPath());
      return null;
    }

    @Override
    public Void visitMemberSelect(MemberSelectTree t, Void v) {
      raw(getCurrentPath());
      return super.visitMemberSelect(t, v);
    }

    @Override
    public Void visitMethodInvocation(MethodInvocationTree t, Void v) {
      if (start(t) >= 0 && isRaw(trees.getTypeMirror(getCurrentPath()))) any(getCurrentPath(), t, "call_result");
      return super.visitMethodInvocation(t, v);
    }

    @Override
    public Void visitTypeCast(TypeCastTree t, Void v) {
      cast(getCurrentPath(), t);
      return super.visitTypeCast(t, v);
    }

    @Override
    public Void visitThrow(ThrowTree t, Void v) {
      thrown(getCurrentPath(), t);
      return super.visitThrow(t, v);
    }

    @Override
    public Void visitCatch(CatchTree t, Void v) {
      caught(getCurrentPath(), t);
      return super.visitCatch(t, v);
    }

    @Override
    public Void visitExpressionStatement(ExpressionStatementTree t, Void v) {
      floating(getCurrentPath(), t);
      return super.visitExpressionStatement(t, v);
    }
  }

  // ── suppressions ───────────────────────────────────────────────────────────

  /**
   * `@SuppressWarnings` — a row per tool its rules name — and SpotBugs'
   * `@SuppressFBWarnings`, judged by its simple name when it did not resolve.
   */
  private void suppression(TreePath path, AnnotationTree a) {
    if (start(a) < 0) return;
    Tree type = a.getAnnotationType();
    Element e = trees.getElement(new TreePath(path, type));
    String qualified = e instanceof TypeElement te && !n.unresolved(te) ? te.getQualifiedName().toString() : null;
    String simple = type instanceof MemberSelectTree ms ? ms.getIdentifier().toString() : type.toString();
    long line = line(start(a));
    if ("java.lang.SuppressWarnings".equals(qualified)) {
      Map<String, List<String>> byTool = new LinkedHashMap<>();
      for (String rule : values(path, a)) byTool.computeIfAbsent(suppressor(rule), k -> new ArrayList<>()).add(rule);
      for (Map.Entry<String, List<String>> t : byTool.entrySet()) lint(line, t.getKey(), "SuppressWarnings", String.join(",", t.getValue()));
    } else if (qualified != null ? qualified.equals("edu.umd.cs.findbugs.annotations.SuppressFBWarnings") : simple.equals("SuppressFBWarnings")) {
      lint(line, "spotbugs", "SuppressFBWarnings", String.join(",", values(path, a)));
    }
  }

  /** Which tool reads a `@SuppressWarnings` rule; javac and the tools reading its own names take the rest. */
  static String suppressor(String rule) {
    if (rule.startsWith("checkstyle:")) return "checkstyle";
    if (rule.startsWith("java:") || rule.startsWith("squid:")) return "sonar";
    if (rule.equals("PMD") || rule.startsWith("PMD.")) return "pmd";
    return "javac";
  }

  /** The strings an annotation's `value` holds: literals, and constants javac resolved. */
  private List<String> values(TreePath path, AnnotationTree a) {
    List<String> out = new ArrayList<>();
    for (ExpressionTree arg : a.getArguments()) {
      ExpressionTree value = arg;
      if (arg instanceof AssignmentTree as) {
        if (!(as.getVariable() instanceof IdentifierTree id && id.getName().contentEquals("value"))) continue;
        value = as.getExpression();
      }
      List<? extends ExpressionTree> items = value instanceof NewArrayTree na && na.getInitializers() != null ? na.getInitializers() : List.of(value);
      for (ExpressionTree item : items) {
        if (item instanceof LiteralTree l && l.getValue() instanceof String str) {
          out.add(str);
        } else if (trees.getElement(new TreePath(path, item)) instanceof VariableElement ve && ve.getConstantValue() instanceof String str) {
          out.add(str);
        }
      }
    }
    return out;
  }

  // ── types ──────────────────────────────────────────────────────────────────

  /**
   * A raw type written where javac's rawtypes lint looks: not a qualifier, a
   * class literal, a cast or `instanceof`, and not what javac infers for `var`
   * (a type tree with no end in the source).
   */
  private void raw(TreePath path) {
    Tree leaf = path.getLeaf();
    if (start(leaf) < 0 || pos.getEndPosition(cu, leaf) < 0) return;
    Tree parent = path.getParentPath() == null ? null : path.getParentPath().getLeaf();
    if (parent instanceof ClassTree body && path.getParentPath().getParentPath() != null
        && path.getParentPath().getParentPath().getLeaf() instanceof NewClassTree nc && nc.getClassBody() == body) {
      return; // an anonymous class's supertype is the `new`'s own name, reached there
    }
    if (!(trees.getElement(path) instanceof TypeElement) || !isRaw(trees.getTypeMirror(path))) return;
    if (parent instanceof ParameterizedTypeTree pt && pt.getType() == leaf) return;
    if (parent instanceof MemberSelectTree ms && ms.getExpression() == leaf) return;
    if (parent instanceof MemberReferenceTree mr && mr.getQualifierExpression() == leaf) return;
    if (parent instanceof AnnotationTree) return;
    for (TreePath p = path; p.getParentPath() != null; p = p.getParentPath()) {
      Tree up = p.getParentPath().getLeaf();
      if (up instanceof TypeCastTree c) {
        if (c.getType() == p.getLeaf()) return;
        break;
      }
      if (up instanceof InstanceOfTree) return;
      if (!(up instanceof ArrayTypeTree || up instanceof AnnotatedTypeTree)) break;
    }
    any(path, leaf, "raw");
  }

  /** A generic class used without its type arguments. */
  private static boolean isRaw(TypeMirror t) {
    return t != null && t.getKind() == TypeKind.DECLARED && t instanceof DeclaredType dt && dt.getTypeArguments().isEmpty()
        && dt.asElement() instanceof TypeElement te && !te.getTypeParameters().isEmpty();
  }

  private void any(TreePath path, Tree at, String kind) {
    x.em.emit("any_site", Main.row("fn", n.owner(s, path), "file", s.path, "line", line(start(at)), "kind", kind));
  }

  private void cast(TreePath path, TypeCastTree c) {
    if (start(c) < 0) return;
    x.em.emit("assertion", Main.row(
        "fn", n.owner(s, path), "file", s.path, "line", line(start(c)), "kind", "cast", "to_type", Text.truncate(source(c.getType()), 200),
        "from_any", isRaw(trees.getTypeMirror(new TreePath(path, c.getExpression()))), "to_any", isRaw(trees.getTypeMirror(new TreePath(path, c.getType())))));
  }

  // ── values ─────────────────────────────────────────────────────────────────

  /** A string, character, text block or number; `true`, `false` and `null` are no literals. */
  private void literal(TreePath path, LiteralTree l) {
    if (start(l) < 0) return;
    String kind;
    String value;
    switch (l.getKind()) {
      case STRING_LITERAL -> {
        kind = "string";
        value = (String) l.getValue();
      }
      case CHAR_LITERAL -> {
        kind = "string";
        value = String.valueOf(l.getValue());
      }
      case INT_LITERAL, LONG_LITERAL, FLOAT_LITERAL, DOUBLE_LITERAL -> {
        kind = "number";
        value = source(l);
      }
      default -> {
        return;
      }
    }
    x.em.emit("literal", Main.row("fn", n.owner(s, path), "file", s.path, "line", line(start(l)), "kind", kind, "value", Text.truncate(value, 100)));
  }

  // ── exceptions ─────────────────────────────────────────────────────────────

  /** A `throw`, typed by what javac says the thrown expression is. */
  private void thrown(TreePath path, ThrowTree t) {
    if (start(t) < 0) return;
    TypeMirror type = trees.getTypeMirror(new TreePath(path, t.getExpression()));
    String id = null;
    if (type != null && type.getKind() == TypeKind.DECLARED && n.types.asElement(type) instanceof TypeElement te && !n.unresolved(te)) id = n.idOf(te);
    x.em.emit("throw_site", Main.row("fn", n.owner(s, path), "file", s.path, "line", line(start(t)), "type", id));
  }

  private void caught(TreePath path, CatchTree c) {
    if (start(c) < 0) return;
    String name = c.getParameter().getName().toString();
    boolean[] rethrows = {false};
    new TreeScanner<Void, Void>() {
      @Override
      public Void visitThrow(ThrowTree t, Void v) {
        rethrows[0] = true;
        return null;
      }

      @Override
      public Void visitLambdaExpression(LambdaExpressionTree t, Void v) {
        return null;
      }

      @Override
      public Void visitClass(ClassTree t, Void v) {
        return null;
      }
    }.scan(c.getBlock(), null);
    x.em.emit("catch_site", Main.row(
        "fn", n.owner(s, path), "file", s.path, "line", line(start(c)), "binds", !name.isEmpty() && !name.equals("_"),
        "empty", c.getBlock().getStatements().isEmpty(), "rethrows", rethrows[0]));
  }

  /** A call made as a statement whose `Future` or `CompletionStage` nothing keeps. */
  private void floating(TreePath path, ExpressionStatementTree st) {
    ExpressionTree e = st.getExpression();
    TreePath at = new TreePath(path, e);
    while (e instanceof ParenthesizedTree p) {
      e = p.getExpression();
      at = new TreePath(at, e);
    }
    if (!(e instanceof MethodInvocationTree || e instanceof NewClassTree) || start(e) < 0) return;
    TypeMirror type = trees.getTypeMirror(at);
    if (type == null || !n.promise(type)) return;
    int id = x.callSiteId(s, start(e), pos.getEndPosition(cu, e));
    x.em.emit("floating_promise", Main.row("call_site", id, "fn", n.owner(s, path), "file", s.path, "line", line(start(e))));
  }

  // ── where ──────────────────────────────────────────────────────────────────

  private long start(Tree t) {
    return pos.getStartPosition(cu, t);
  }

  private long line(long offset) {
    return lines.getLineNumber(Math.max(offset, 0));
  }

  private String source(Tree t) {
    long a = pos.getStartPosition(cu, t);
    long b = pos.getEndPosition(cu, t);
    return a < 0 || b < a ? t.toString() : s.text.substring((int) a, (int) b);
  }
}

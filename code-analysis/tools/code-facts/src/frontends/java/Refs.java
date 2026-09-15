package codefacts;

import com.sun.source.tree.AnnotationTree;
import com.sun.source.tree.AssignmentTree;
import com.sun.source.tree.BlockTree;
import com.sun.source.tree.ClassTree;
import com.sun.source.tree.CompilationUnitTree;
import com.sun.source.tree.CompoundAssignmentTree;
import com.sun.source.tree.IdentifierTree;
import com.sun.source.tree.ImportTree;
import com.sun.source.tree.InstanceOfTree;
import com.sun.source.tree.LambdaExpressionTree;
import com.sun.source.tree.LineMap;
import com.sun.source.tree.MemberReferenceTree;
import com.sun.source.tree.MemberSelectTree;
import com.sun.source.tree.MethodInvocationTree;
import com.sun.source.tree.MethodTree;
import com.sun.source.tree.NewClassTree;
import com.sun.source.tree.PackageTree;
import com.sun.source.tree.ParenthesizedTree;
import com.sun.source.tree.Tree;
import com.sun.source.tree.TypeCastTree;
import com.sun.source.tree.UnaryTree;
import com.sun.source.tree.VariableTree;
import com.sun.source.util.DocTrees;
import com.sun.source.util.SourcePositions;
import com.sun.source.util.TreePath;
import com.sun.source.util.TreePathScanner;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import javax.lang.model.element.Element;
import javax.lang.model.element.ElementKind;
import javax.lang.model.element.ExecutableElement;
import javax.lang.model.element.Modifier;
import javax.lang.model.element.TypeElement;
import javax.lang.model.type.DeclaredType;
import javax.lang.model.type.TypeKind;
import javax.lang.model.type.TypeMirror;

/**
 * The refs layer for one file, once its source set is analysed: every name javac
 * resolved as a `ref` from its enclosing declaration; every call with its target
 * and how it dispatches; the supertypes written, and the supertype members each
 * method overrides; member access, type positions, declared exceptions and
 * types.
 */
final class Refs {
  private final Extractor x;
  private final Names n;
  private final Extractor.Source s;
  private final CompilationUnitTree cu;
  private final DocTrees trees;
  private final SourcePositions pos;
  private final LineMap lines;
  private final Set<String> typed = new HashSet<>();
  private final Map<TypeElement, Boolean> functional = new HashMap<>();

  Refs(Extractor x, Names n, Extractor.Source s) {
    this.x = x;
    this.n = n;
    this.s = s;
    this.cu = s.cu;
    this.trees = n.trees;
    this.pos = trees.getSourcePositions();
    this.lines = cu.getLineMap();
  }

  void run() {
    new Scanner().scan(new TreePath(cu), null);
  }

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
    public Void visitClass(ClassTree t, Void v) {
      hierarchy(getCurrentPath(), t);
      return super.visitClass(t, v);
    }

    @Override
    public Void visitMethod(MethodTree t, Void v) {
      // What the compiler writes — a default constructor, a record's accessors — is no one's reference.
      if (trees.getElement(getCurrentPath()) instanceof ExecutableElement m && n.elements.getOrigin(m) != javax.lang.model.util.Elements.Origin.EXPLICIT) return null;
      String fn = idOf(t);
      if (fn != null) {
        for (Tree thrown : t.getThrows()) {
          Element e = trees.getElement(new TreePath(getCurrentPath(), thrown));
          if (e != null && !n.unresolved(e)) x.em.emit("throws_decl", Main.row("fn", fn, "type", n.idOf(e)));
        }
      }
      symbolType(getCurrentPath(), t);
      return super.visitMethod(t, v);
    }

    @Override
    public Void visitVariable(VariableTree t, Void v) {
      symbolType(getCurrentPath(), t);
      return super.visitVariable(t, v);
    }

    @Override
    public Void visitIdentifier(IdentifierTree t, Void v) {
      name(getCurrentPath(), t.getName().toString());
      return null;
    }

    @Override
    public Void visitMemberSelect(MemberSelectTree t, Void v) {
      name(getCurrentPath(), t.getIdentifier().toString());
      return super.visitMemberSelect(t, v);
    }

    @Override
    public Void visitMemberReference(MemberReferenceTree t, Void v) {
      name(getCurrentPath(), t.getMode() == MemberReferenceTree.ReferenceMode.NEW ? "new" : t.getName().toString());
      return super.visitMemberReference(t, v);
    }

    @Override
    public Void visitMethodInvocation(MethodInvocationTree t, Void v) {
      call(getCurrentPath(), t);
      return super.visitMethodInvocation(t, v);
    }

    @Override
    public Void visitNewClass(NewClassTree t, Void v) {
      call(getCurrentPath(), t);
      return super.visitNewClass(t, v);
    }
  }

  // ── names ──────────────────────────────────────────────────────────────────

  private void name(TreePath path, String name) {
    Tree leaf = path.getLeaf();
    if (!(leaf instanceof MemberReferenceTree) && (name.equals("this") || name.equals("super") || name.equals("class")) || start(leaf) < 0) return;
    Element e = trees.getElement(path);
    if (e == null || n.unresolved(e)) {
      unresolved(path, name, e);
      return;
    }
    if (Names.local(e)) return;
    String to = n.idOf(e);
    if (to == null) return;
    String kind = kind(path, e);
    String from = owner(path);
    long line = line(start(leaf));
    x.em.emit("ref", Main.row("from", from, "to", to, "kind", kind, "file", s.path, "line", line));
    if (kind.equals("type")) x.em.emit("type_ref", Main.row("from", from, "to", to, "position", position(path), "file", s.path, "line", line));
    if (kind.equals("read") || kind.equals("write") || kind.equals("readwrite") || kind.equals("call") || kind.equals("value")) {
      memberAccess(path, e, to, kind, from, line);
    }
  }

  /** How a name is mentioned, judged at the expression it heads. */
  private String kind(TreePath path, Element e) {
    Tree leaf = path.getLeaf();
    TreePath head = path;
    while (head.getParentPath() != null) {
      Tree p = head.getParentPath().getLeaf();
      if (p instanceof com.sun.source.tree.ParameterizedTypeTree pt && pt.getType() == head.getLeaf()) head = head.getParentPath();
      else if (p instanceof com.sun.source.tree.AnnotatedTypeTree at && at.getUnderlyingType() == head.getLeaf()) head = head.getParentPath();
      else break;
    }
    Tree h = head.getLeaf();
    Tree parent = head.getParentPath() == null ? null : head.getParentPath().getLeaf();
    if (leaf instanceof MemberReferenceTree) return "value";
    if (parent instanceof ClassTree c) {
      if (c.getExtendsClause() == h) return "extends";
      if (c.getImplementsClause().contains(h)) return c.getKind() == Tree.Kind.INTERFACE ? "extends" : "implements";
    }
    if (parent instanceof AnnotationTree a && a.getAnnotationType() == h) return "decorator";
    if (parent instanceof NewClassTree nc && nc.getIdentifier() == h) return "new";
    if (parent instanceof MethodInvocationTree mi && mi.getMethodSelect() == leaf) return "call";
    if (parent instanceof MemberSelectTree ms && ms.getExpression() == leaf && ms.getIdentifier().contentEquals("class")) return "value";
    if (e instanceof TypeElement) return "type";
    if (e.getKind().isField() || e.getKind() == ElementKind.RECORD_COMPONENT || e instanceof ExecutableElement) return access(path);
    return "read";
  }

  /** read, write or readwrite, by the assignment the expression is the target of. */
  private String access(TreePath path) {
    TreePath p = path;
    while (p.getParentPath() != null && p.getParentPath().getLeaf() instanceof ParenthesizedTree) p = p.getParentPath();
    Tree node = p.getLeaf();
    Tree parent = p.getParentPath() == null ? null : p.getParentPath().getLeaf();
    if (parent instanceof AssignmentTree a && a.getVariable() == node) return "write";
    if (parent instanceof CompoundAssignmentTree a && a.getVariable() == node) return "readwrite";
    if (parent instanceof UnaryTree u) {
      switch (u.getKind()) {
        case PREFIX_INCREMENT, PREFIX_DECREMENT, POSTFIX_INCREMENT, POSTFIX_DECREMENT -> {
          return "readwrite";
        }
        default -> {}
      }
    }
    return "read";
  }

  /** Where a type is written, in `type_ref`'s vocabulary. */
  private String position(TreePath path) {
    Tree prev = path.getLeaf();
    for (TreePath p = path.getParentPath(); p != null; prev = p.getLeaf(), p = p.getParentPath()) {
      Tree t = p.getLeaf();
      if (t instanceof VariableTree v) {
        if (v.getType() != prev) return "other";
        Tree owner = p.getParentPath() == null ? null : p.getParentPath().getLeaf();
        if (owner instanceof MethodTree || owner instanceof LambdaExpressionTree) return "param";
        if (owner instanceof ClassTree) return "property";
        return "variable";
      }
      if (t instanceof MethodTree m) return m.getReturnType() == prev ? "return" : "other";
      if (t instanceof ClassTree c) {
        if (c.getExtendsClause() == prev) return "extends";
        if (c.getImplementsClause().contains(prev)) return c.getKind() == Tree.Kind.INTERFACE ? "extends" : "implements";
        return "other";
      }
      if (t instanceof NewClassTree nc) return nc.getTypeArguments().contains(prev) || nc.getIdentifier() == prev ? "type_arg" : "other";
      if (t instanceof MethodInvocationTree mi) return mi.getTypeArguments().contains(prev) ? "type_arg" : "other";
      if (t instanceof TypeCastTree c) return c.getType() == prev ? "assertion" : "other";
      if (t instanceof InstanceOfTree) return "assertion";
      if (t instanceof com.sun.source.tree.StatementTree || t instanceof com.sun.source.tree.ExpressionTree && !(t instanceof MemberSelectTree)
          && !(t instanceof com.sun.source.tree.ParameterizedTypeTree) && !(t instanceof com.sun.source.tree.AnnotatedTypeTree)
          && !(t instanceof com.sun.source.tree.ArrayTypeTree) && !(t instanceof com.sun.source.tree.WildcardTree)) {
        return "other";
      }
    }
    return "other";
  }

  /** A function touching a field or method of a project class or interface. */
  private void memberAccess(TreePath path, Element e, String to, String kind, String from, long line) {
    Map<String, Object> target = x.symbols.get(to);
    if (target == null || !"project".equals(target.get("origin"))) return;
    if (!"property".equals(target.get("kind")) && !"method".equals(target.get("kind"))) return;
    String ownerId = (String) target.get("parent");
    Map<String, Object> owner = ownerId == null ? null : x.symbols.get(ownerId);
    if (owner == null || !("class".equals(owner.get("kind")) || "interface".equals(owner.get("kind")))) return;
    String mode = kind.equals("call") ? "call" : kind.equals("value") ? "read" : kind;
    Tree leaf = path.getLeaf();
    boolean viaThis;
    if (leaf instanceof MemberSelectTree ms) {
      Tree q = ms.getExpression();
      viaThis = q instanceof IdentifierTree id && (id.getName().contentEquals("this") || id.getName().contentEquals("super"))
          || q instanceof MemberSelectTree qs && qs.getIdentifier().contentEquals("this");
    } else {
      // An unqualified instance member is `this`'s.
      viaThis = leaf instanceof IdentifierTree && !e.getModifiers().contains(Modifier.STATIC);
    }
    x.em.emit("member_access", Main.row("fn", from, "member", to, "owner", ownerId, "mode", mode, "via_this", viaThis, "file", s.path, "line", line));
  }

  private void unresolved(TreePath path, String name, Element e) {
    Tree leaf = path.getLeaf();
    if (e instanceof javax.lang.model.element.PackageElement) return;
    // A member of what could not be typed resolves to nothing by design.
    if (leaf instanceof MemberSelectTree ms) {
      TypeMirror q = trees.getTypeMirror(new TreePath(path, ms.getExpression()));
      if (q == null || q.getKind() == TypeKind.ERROR || q.getKind() == TypeKind.PACKAGE) return;
    }
    Tree parent = path.getParentPath() == null ? null : path.getParentPath().getLeaf();
    String kind = parent instanceof MethodInvocationTree mi && mi.getMethodSelect() == leaf ? "call"
        : parent instanceof NewClassTree ? "call"
        : e != null && e.getKind() != ElementKind.OTHER || typePosition(path) ? "type" : "read";
    x.em.emit("unresolved_ref", Main.row("from", owner(path), "name", Text.truncate(name, 100), "kind", kind, "file", s.path, "line", line(start(leaf))));
  }

  private boolean typePosition(TreePath path) {
    Tree parent = path.getParentPath() == null ? null : path.getParentPath().getLeaf();
    return parent instanceof VariableTree v && v.getType() == path.getLeaf() || parent instanceof MethodTree m && m.getReturnType() == path.getLeaf()
        || parent instanceof com.sun.source.tree.ParameterizedTypeTree || parent instanceof TypeCastTree;
  }

  // ── calls ──────────────────────────────────────────────────────────────────

  /**
   * A call: a constructor, a static or private method, and a `super.` call are
   * `static`; any other method `virtual`, at the declaration javac resolved.
   */
  private void call(TreePath path, Tree t) {
    long start = start(t);
    if (start < 0) return;
    int id = x.callSiteId(s, start, pos.getEndPosition(cu, t));
    Element e = trees.getElement(path);
    String kind = "call";
    String name = null;
    int args;
    boolean superCall = false;
    if (t instanceof NewClassTree nc) {
      kind = "new";
      name = source(nc.getIdentifier()).replaceAll("\\s+", "");
      int lt = name.indexOf('<');
      if (lt >= 0) name = name.substring(0, lt);
      args = nc.getArguments().size();
    } else {
      MethodInvocationTree mi = (MethodInvocationTree) t;
      Tree select = mi.getMethodSelect();
      if (select instanceof IdentifierTree ident) {
        name = ident.getName().toString();
        if (name.equals("super")) kind = "super";
      } else if (select instanceof MemberSelectTree ms) {
        name = ms.getIdentifier().toString();
        superCall = ms.getExpression() instanceof IdentifierTree q && q.getName().contentEquals("super");
      }
      args = mi.getArguments().size();
    }
    String callee = null;
    String dispatch = "unresolved";
    if (e instanceof ExecutableElement m && !n.unresolved(m)) {
      callee = n.idOf(m);
      boolean isStatic = m.getKind() == ElementKind.CONSTRUCTOR || m.getModifiers().contains(Modifier.STATIC) || m.getModifiers().contains(Modifier.PRIVATE);
      dispatch = callee == null ? "unresolved" : isStatic || superCall || !kind.equals("call") ? "static" : "virtual";
    }
    x.em.emit("call_site", Main.row(
        "id", id, "caller", owner(path), "callee", callee, "callee_name", name == null ? null : Text.truncate(name, 100),
        "dispatch", dispatch, "kind", kind, "file", s.path, "line", line(start), "col", lines.getColumnNumber(start),
        "args", args, "awaited", false, "optional", false, "spread", false));
  }

  // ── types ──────────────────────────────────────────────────────────────────

  /** The supertypes a type writes, and each of its methods' overridden members in its direct supertypes. */
  private void hierarchy(TreePath path, ClassTree t) {
    String self = idOf(t);
    if (!(trees.getElement(path) instanceof TypeElement type) || self == null) return;
    boolean iface = type.getKind() == ElementKind.INTERFACE || type.getKind() == ElementKind.ANNOTATION_TYPE;
    if (t.getExtendsClause() != null) supertype(path, t.getExtendsClause(), self, "extends");
    for (Tree i : t.getImplementsClause()) supertype(path, i, self, iface ? "extends" : "implements");

    List<? extends TypeMirror> supers = n.types.directSupertypes(type.asType());
    for (Tree member : t.getMembers()) {
      if (!(member instanceof MethodTree mt)) continue;
      if (!(trees.getElement(new TreePath(path, mt)) instanceof ExecutableElement m) || m.getKind() != ElementKind.METHOD || m.getModifiers().contains(Modifier.STATIC)) continue;
      String memberId = idOf(mt);
      if (memberId == null) continue;
      for (TypeMirror sup : supers) {
        if (!(n.types.asElement(sup) instanceof TypeElement st)) continue;
        if (iface && st.getQualifiedName().contentEquals("java.lang.Object")) continue;
        for (Element base : n.elements.getAllMembers(st)) {
          if (base instanceof ExecutableElement b && b.getKind() == ElementKind.METHOD && b.getSimpleName().equals(m.getSimpleName())
              && !b.getModifiers().contains(Modifier.STATIC) && n.elements.overrides(m, b, type)) {
            String baseId = n.idOf(b);
            if (baseId != null && !baseId.equals(memberId)) x.em.emit("overrides", Main.row("member", memberId, "base", baseId));
          }
        }
      }
    }
  }

  private void supertype(TreePath path, Tree written, String self, String relation) {
    TypeMirror tm = trees.getTypeMirror(new TreePath(path, written));
    Element e = tm == null ? null : n.types.asElement(tm);
    if (e == null || n.unresolved(e)) return;
    String parent = n.idOf(e);
    if (parent == null) return;
    if (relation.equals("extends")) x.em.emit("extends", Main.row("child", self, "parent", parent));
    else x.em.emit("implements", Main.row("class", self, "interface", parent, "pointer", null));
  }

  /** A value-carrying declaration's type, as javac prints it. */
  private void symbolType(TreePath path, Tree decl) {
    String id = idOf(decl);
    if (id == null || !typed.add(id)) return;
    Element e = trees.getElement(path);
    if (e == null) return;
    TypeMirror t = e.asType();
    if (t == null || t.getKind() == TypeKind.ERROR) return;
    boolean method = e instanceof ExecutableElement;
    x.em.emit("symbol_type", Main.row(
        "symbol", id, "text", Text.truncate(t.toString(), 200), "is_any", false, "is_unknown", false,
        "is_promise", !method && promise(t), "is_function", method || functional(t), "is_union", t.getKind() == TypeKind.UNION));
  }

  private boolean promise(TypeMirror t) {
    if (t.getKind() != TypeKind.DECLARED) return false;
    for (String name : List.of("java.util.concurrent.Future", "java.util.concurrent.CompletionStage")) {
      TypeElement te = n.elements.getTypeElement(name);
      if (te != null && n.types.isAssignable(n.types.erasure(t), n.types.erasure(te.asType()))) return true;
    }
    return false;
  }

  /** A functional interface: exactly one abstract method that is not one of Object's. */
  private boolean functional(TypeMirror t) {
    if (!(t instanceof DeclaredType dt) || !(dt.asElement() instanceof TypeElement te) || te.getKind() != ElementKind.INTERFACE) return false;
    return functional.computeIfAbsent(te, k -> {
      TypeElement object = n.elements.getTypeElement("java.lang.Object");
      int abstracts = 0;
      for (Element m : n.elements.getAllMembers(k)) {
        if (!(m instanceof ExecutableElement ex) || !ex.getModifiers().contains(Modifier.ABSTRACT)) continue;
        boolean objects = false;
        if (object != null) {
          for (Element om : object.getEnclosedElements()) {
            if (om instanceof ExecutableElement o && o.getModifiers().contains(Modifier.PUBLIC) && n.elements.overrides(ex, o, k)) objects = true;
          }
        }
        if (!objects) abstracts++;
      }
      return abstracts == 1;
    });
  }

  // ── where ──────────────────────────────────────────────────────────────────

  /**
   * The innermost enclosing declaration with a name — what a reference is from:
   * a method, constructor, lambda, class, field (for its initializer) or
   * initializer block, else the file's `<module>`.
   */
  private String owner(TreePath path) {
    for (TreePath p = path.getParentPath(); p != null; p = p.getParentPath()) {
      Tree t = p.getLeaf();
      Tree parent = p.getParentPath() == null ? null : p.getParentPath().getLeaf();
      boolean named = t instanceof MethodTree || t instanceof LambdaExpressionTree || t instanceof ClassTree
          || (t instanceof VariableTree || t instanceof BlockTree) && parent instanceof ClassTree;
      if (!named) continue;
      String id = idOf(t);
      if (id != null) return id;
    }
    return s.path + "#<module>";
  }

  private String idOf(Tree t) {
    return x.idByKey.get(Extractor.key(s.abs, start(t), t));
  }

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

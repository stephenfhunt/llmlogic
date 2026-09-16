package codefacts;

import com.sun.source.tree.ArrayAccessTree;
import com.sun.source.tree.AssertTree;
import com.sun.source.tree.AssignmentTree;
import com.sun.source.tree.BinaryTree;
import com.sun.source.tree.BindingPatternTree;
import com.sun.source.tree.BlockTree;
import com.sun.source.tree.CaseTree;
import com.sun.source.tree.CatchTree;
import com.sun.source.tree.ClassTree;
import com.sun.source.tree.CompilationUnitTree;
import com.sun.source.tree.CompoundAssignmentTree;
import com.sun.source.tree.ConditionalExpressionTree;
import com.sun.source.tree.DoWhileLoopTree;
import com.sun.source.tree.EnhancedForLoopTree;
import com.sun.source.tree.ExpressionStatementTree;
import com.sun.source.tree.ExpressionTree;
import com.sun.source.tree.ForLoopTree;
import com.sun.source.tree.IdentifierTree;
import com.sun.source.tree.IfTree;
import com.sun.source.tree.InstanceOfTree;
import com.sun.source.tree.LabeledStatementTree;
import com.sun.source.tree.LambdaExpressionTree;
import com.sun.source.tree.LineMap;
import com.sun.source.tree.MemberReferenceTree;
import com.sun.source.tree.MemberSelectTree;
import com.sun.source.tree.MethodInvocationTree;
import com.sun.source.tree.MethodTree;
import com.sun.source.tree.NewArrayTree;
import com.sun.source.tree.NewClassTree;
import com.sun.source.tree.ParenthesizedTree;
import com.sun.source.tree.ReturnTree;
import com.sun.source.tree.StatementTree;
import com.sun.source.tree.SwitchExpressionTree;
import com.sun.source.tree.SwitchTree;
import com.sun.source.tree.SynchronizedTree;
import com.sun.source.tree.ThrowTree;
import com.sun.source.tree.Tree;
import com.sun.source.tree.TryTree;
import com.sun.source.tree.TypeCastTree;
import com.sun.source.tree.UnaryTree;
import com.sun.source.tree.VariableTree;
import com.sun.source.tree.WhileLoopTree;
import com.sun.source.tree.YieldTree;
import com.sun.source.util.DocTrees;
import com.sun.source.util.SourcePositions;
import com.sun.source.util.TreePath;
import java.util.ArrayDeque;
import java.util.Deque;
import java.util.List;
import java.util.Map;
import javax.lang.model.element.Element;
import javax.lang.model.element.ElementKind;
import javax.lang.model.element.ExecutableElement;
import javax.lang.model.element.Modifier;
import javax.lang.model.element.RecordComponentElement;
import javax.lang.model.element.TypeElement;
import javax.lang.model.util.Elements;
import javax.lang.model.type.TypeKind;
import javax.lang.model.type.TypeMirror;

/**
 * The dataflow layer for one file, once its source set is analysed: every
 * expression in three-address normal form, as Doop-style input facts —
 * `assign`, `alloc`, `load`, `store`, and the formal/actual parameter and return
 * slots that join methods at call sites. Flow-insensitive (statement order is
 * the flow layer's business), field-based (a field is its name),
 * context-insensitive. `lib/pointsto.dl` computes points-to and the call graph
 * of calls through a value from these; `lib/taint.dl` follows values.
 *
 * <p>The model is the TypeScript layer's, so the library reads either. Named
 * variables are their symbol ids, so a value flows across files with no extra
 * fact; everything else — a subexpression's value, a method's `this` and return
 * slot — is named after the declaration it belongs to. What Java adds:
 *
 * <ul>
 *   <li>a field read by its simple name is a `load` from `this` (or from the
 *       class, for a static), since Java writes no receiver;
 *   <li>a lambda and a method reference are `function` allocations; a call of a
 *       functional interface's own method names its receiver `callee_var` too,
 *       which is what resolves it to the lambda;
 *   <li>a record the compiler wrote a canonical constructor and accessors for is
 *       modelled at the sites that *are* written: `new R(a, b)` stores each
 *       argument into its component, and `r.x()` loads it back;
 *   <li>type patterns bind by copy, record patterns by a load per component.
 * </ul>
 *
 * <p>Not modelled: a bound method reference's receiver (`obj::m`); reflection;
 * element flow through streams and `Optional` beyond `[]`.
 */
final class Dataflow {
  /** Every thrown value, in every function: the catch that binds it reads this. */
  private static final String THROWN = "$thrown";

  private final Extractor x;
  private final Names n;
  private final Extractor.Source s;
  private final CompilationUnitTree cu;
  private final DocTrees trees;
  private final SourcePositions pos;
  private final LineMap lines;

  Dataflow(Extractor x, Names n, Extractor.Source s) {
    this.x = x;
    this.n = n;
    this.s = s;
    this.cu = s.cu;
    this.trees = n.trees;
    this.pos = trees.getSourcePositions();
    this.lines = cu.getLineMap();
  }

  void run() {
    TreePath root = new TreePath(cu);
    for (Tree t : cu.getTypeDecls()) {
      if (t instanceof ClassTree c) type(new TreePath(root, c));
    }
  }

  // ── owners ─────────────────────────────────────────────────────────────────

  /**
   * Each member of a class is its own owner, as it is on the refs layer: a
   * method, a constructor, a field (for its initializer), an initializer block,
   * a nested type.
   */
  private void type(TreePath path) {
    ClassTree c = (ClassTree) path.getLeaf();
    String cls = idOf(c);
    if (cls == null) return;
    statics(path, c, cls);
    for (Tree m : c.getMembers()) {
      TreePath mp = new TreePath(path, m);
      if (!written(mp)) continue;
      if (m instanceof ClassTree) type(mp);
      else if (m instanceof MethodTree mt) method(mp, mt, cls);
      else if (m instanceof VariableTree vt) field(mp, vt, cls);
      else if (m instanceof BlockTree bt) initializer(mp, bt, cls);
    }
  }

  /**
   * A class holding static state is a `cell`, the object its static fields are
   * fields of — as a Go package-level variable is. Its own class has to allocate
   * it, once, so that every file reading `C.f` loads from the same object.
   */
  private void statics(TreePath path, ClassTree c, String cls) {
    boolean any = false;
    for (Tree m : c.getMembers()) {
      TreePath mp = new TreePath(path, m);
      if (!(m instanceof VariableTree) || !written(mp)) continue;
      Element e = trees.getElement(mp);
      if (e != null && (e.getModifiers().contains(Modifier.STATIC) || e.getKind() == ElementKind.ENUM_CONSTANT)) any = true;
    }
    if (!any) return;
    Map<String, Object> row = x.symbols.get(cls);
    Object parent = row == null ? null : row.get("parent");
    x.declareVar(cls, parent instanceof String p ? p : s.path + "#<module>", "module");
    x.em.emit("alloc", Main.row(
        "var", cls, "site", x.nextAllocSite++, "kind", "cell", "type", null, "fn_target", null,
        "file", s.path, "line", line(start(c))));
  }

  /** What the compiler writes is no one's: a default constructor, a record's accessors and fields. */
  private boolean written(TreePath path) {
    Element e = trees.getElement(path);
    return e == null || n.elements.getOrigin(e) == Elements.Origin.EXPLICIT;
  }

  /**
   * Whether a member is written in source. A record's canonical constructor is
   * `MANDATED` and carries a tree javac made up; its accessors are neither —
   * javac calls them explicit and gives them no declaration at all — so the test
   * that catches both is a declaration of one's own.
   */
  private boolean declared(Element e) {
    TreePath p = trees.getPath(e);
    return p != null && p.getLeaf() instanceof MethodTree && n.elements.getOrigin(e) == Elements.Origin.EXPLICIT;
  }

  private void method(TreePath path, MethodTree t, String cls) {
    String fn = idOf(t);
    if (fn == null || t.getBody() == null) return;
    boolean isStatic = t.getModifiers().getFlags().contains(Modifier.STATIC);
    Body b = new Body(fn, cls, isStatic);
    b.formals(t.getParameters());
    if (!isStatic && b.thisVar != null) x.em.emit("this_var", Main.row("fn", fn, "var", b.thisVar));
    b.statement(new TreePath(path, t.getBody()));
  }

  /** A field's initializer stores into the new instance, or into the class for a static. */
  private void field(TreePath path, VariableTree t, String cls) {
    String id = idOf(t);
    if (id == null || t.getInitializer() == null) return;
    Element e = trees.getElement(path);
    boolean isStatic = e == null ? t.getModifiers().getFlags().contains(Modifier.STATIC) : e.getModifiers().contains(Modifier.STATIC);
    Body b = new Body(id, cls, isStatic);
    String base = isStatic ? b.classVar() : b.thisVar;
    b.store(base, t.getName().toString(), b.value(new TreePath(path, t.getInitializer())));
  }

  private void initializer(TreePath path, BlockTree t, String cls) {
    String id = idOf(t);
    if (id == null) return;
    new Body(id, cls, t.isStatic()).statement(path);
  }

  // ── a body, in three-address form ──────────────────────────────────────────

  private final class Body {
    /** The declaration every row of this body belongs to, as `Names.owner` names it. */
    private final String fn;
    /** The class the body is written in; null outside one. */
    private final String cls;
    private final boolean isStatic;
    /** `this` here, or null in a static context. */
    private final String thisVar;
    private int temps;
    private boolean retEmitted;
    /** The temporaries the switch expressions being lowered assign into. */
    private final Deque<String> yields = new ArrayDeque<>();

    Body(String fn, String cls, boolean isStatic) {
      this.fn = fn;
      this.cls = cls;
      this.isStatic = isStatic;
      this.thisVar = cls == null || isStatic ? null : declare(cls + "$this", cls, "this");
    }

    /** A lambda's body: its own declaration, but `this` and the class are lexically the outer one's. */
    private Body(String fn, Body outer) {
      this.fn = fn;
      this.cls = outer.cls;
      this.isStatic = outer.isStatic;
      this.thisVar = outer.thisVar;
    }

    // ── rows ─────────────────────────────────────────────────────────────────

    private String declare(String id, String owner, String kind) {
      x.declareVar(id, owner, kind);
      return id;
    }

    private String temp() {
      return declare(fn + "$t" + ++temps, fn, "temp");
    }

    private String ret() {
      String r = fn + "$ret";
      if (!retEmitted) {
        retEmitted = true;
        declare(r, fn, "ret");
        x.em.emit("formal_ret", Main.row("fn", fn, "var", r));
      }
      return r;
    }

    /** The class as a variable: the base its static fields load and store through. */
    private String classVar() {
      if (cls == null) return null;
      Map<String, Object> row = x.symbols.get(cls);
      Object parent = row == null ? null : row.get("parent");
      return declare(cls, parent instanceof String p ? p : s.path + "#<module>", "module");
    }

    private String classVar(TypeElement owner) {
      String id = n.idOf(owner);
      if (id == null) return null;
      Map<String, Object> row = x.symbols.get(id);
      Object parent = row == null ? null : row.get("parent");
      return declare(id, parent instanceof String p ? p : s.path + "#<module>", "module");
    }

    void assign(String to, String from) {
      assign(to, from, "copy");
    }

    void assign(String to, String from, String kind) {
      if (to == null || from == null || to.equals(from)) return;
      x.em.emit("assign", Main.row("to", to, "from", from, "kind", kind, "fn", fn));
    }

    private String load(String base, String field) {
      if (base == null) return null;
      return loadInto(temp(), base, field);
    }

    private String loadInto(String to, String base, String field) {
      if (to == null || base == null) return null;
      x.em.emit("load", Main.row("to", to, "base", base, "field", field, "fn", fn));
      return to;
    }

    void store(String base, String field, String from) {
      if (base == null || from == null) return;
      x.em.emit("store", Main.row("base", base, "field", field, "from", from, "fn", fn));
    }

    private String alloc(Tree at, String kind, String type, String fnTarget) {
      return allocInto(temp(), at, kind, type, fnTarget);
    }

    private String allocInto(String v, Tree at, String kind, String type, String fnTarget) {
      x.em.emit("alloc", Main.row(
          "var", v, "site", x.nextAllocSite++, "kind", kind, "type", type, "fn_target", fnTarget,
          "file", s.path, "line", line(start(at))));
      return v;
    }

    /** A method's or lambda's parameters, in order. */
    void formals(List<? extends VariableTree> params) {
      for (int i = 0; i < params.size(); i++) {
        VariableTree p = params.get(i);
        String id = idOf(p);
        if (id == null) continue;
        declare(id, fn, "param");
        x.em.emit("formal", Main.row("fn", fn, "index", i, "var", id));
      }
    }

    // ── expressions ──────────────────────────────────────────────────────────

    String value(TreePath path, Tree child) {
      return child == null ? null : value(new TreePath(path, child));
    }

    /** The variable holding what an expression evaluates to, if it holds values at all. */
    String value(TreePath path) {
      Tree e = path.getLeaf();
      if (e instanceof ParenthesizedTree p) return value(path, p.getExpression());
      // A cast does not change the object, and neither does `++`/`--`.
      if (e instanceof TypeCastTree c) return value(path, c.getExpression());
      if (e instanceof IdentifierTree || e instanceof MemberSelectTree) return name(path);
      if (e instanceof MethodInvocationTree mi) return call(path, mi);
      if (e instanceof NewClassTree nc) return newClass(path, nc);
      if (e instanceof NewArrayTree na) return newArray(path, na);
      if (e instanceof ArrayAccessTree aa) {
        String base = value(path, aa.getExpression());
        value(path, aa.getIndex());
        return base == null ? null : load(base, "[]");
      }
      if (e instanceof AssignmentTree a) {
        String rhs = value(path, a.getExpression());
        assignTo(new TreePath(path, a.getVariable()), rhs);
        return rhs;
      }
      if (e instanceof CompoundAssignmentTree a) return compound(path, a);
      if (e instanceof ConditionalExpressionTree c) {
        value(path, c.getCondition());
        String t = temp();
        assign(t, value(path, c.getTrueExpression()));
        assign(t, value(path, c.getFalseExpression()));
        return t;
      }
      if (e instanceof BinaryTree b) return binary(path, b);
      if (e instanceof UnaryTree u) return unary(path, u);
      if (e instanceof InstanceOfTree io) {
        String v = value(path, io.getExpression());
        if (io.getPattern() != null) bindPattern(path, io.getPattern(), v);
        return null; // a boolean
      }
      if (e instanceof LambdaExpressionTree l) return lambda(path, l);
      if (e instanceof MemberReferenceTree r) return methodReference(path, r);
      if (e instanceof SwitchExpressionTree sw) return switchExpression(path, sw);
      return null; // a literal, a type, `class`, and the like carry no tracked value
    }

    /** A name: a local's own variable, a field's `load`, `this`, or nothing. */
    private String name(TreePath path) {
      Tree e = path.getLeaf();
      String simple = e instanceof IdentifierTree id ? id.getName().toString() : ((MemberSelectTree) e).getIdentifier().toString();
      if (simple.equals("class")) return null;
      if (simple.equals("this") || simple.equals("super")) return qualifiedThis(path, e);
      Element el = trees.getElement(path);
      if (el == null) {
        // Unresolved, but its qualifier may still allocate or call.
        if (e instanceof MemberSelectTree ms) value(path, ms.getExpression());
        return null;
      }
      switch (el.getKind()) {
        case LOCAL_VARIABLE, PARAMETER, EXCEPTION_PARAMETER, RESOURCE_VARIABLE, BINDING_VARIABLE -> {
          String id = n.projectId(el);
          return id == null ? null : declare(id, ownerOf(id), varKind(el));
        }
        case FIELD, ENUM_CONSTANT, RECORD_COMPONENT -> {
          String base = baseOf(path, el);
          return base == null ? null : load(base, simple);
        }
        default -> {
          // A type or package qualifier: nothing flows, but a qualifier expression may.
          if (e instanceof MemberSelectTree ms && !(el instanceof TypeElement) && el.getKind() != ElementKind.PACKAGE) value(path, ms.getExpression());
          return null;
        }
      }
    }

    /** `this`, `super`, and `Outer.this`. */
    private String qualifiedThis(TreePath path, Tree e) {
      if (e instanceof MemberSelectTree ms) {
        TypeMirror q = trees.getTypeMirror(new TreePath(path, ms.getExpression()));
        if (q != null && n.types.asElement(q) instanceof TypeElement outer) {
          String id = n.idOf(outer);
          if (id != null) return declare(id + "$this", id, "this");
        }
        return thisVar;
      }
      return thisVar;
    }

    /** What a field is read off: the qualifier, `this`, or the class declaring a static. */
    private String baseOf(TreePath path, Element field) {
      Tree e = path.getLeaf();
      if (e instanceof MemberSelectTree ms) {
        Element q = trees.getElement(new TreePath(path, ms.getExpression()));
        // A static field's qualifier is its type, which holds no value of its own.
        if (q instanceof TypeElement owner) return classVar(owner);
        if (q != null && q.getKind() == ElementKind.PACKAGE) return null;
        return value(path, ms.getExpression());
      }
      if (field.getModifiers().contains(Modifier.STATIC) || field.getKind() == ElementKind.ENUM_CONSTANT) {
        TypeElement owner = Names.enclosingType(field);
        return owner == null ? classVar() : classVar(owner);
      }
      return thisVar;
    }

    private String compound(TreePath path, CompoundAssignmentTree a) {
      String rhs = value(path, a.getExpression());
      String cur = value(path, a.getVariable());
      String t = temp();
      assign(t, cur, "derive");
      assign(t, rhs, "derive");
      assignTo(new TreePath(path, a.getVariable()), t);
      return t;
    }

    private String binary(TreePath path, BinaryTree b) {
      String l = value(path, b.getLeftOperand());
      String r = value(path, b.getRightOperand());
      if (!DERIVING.contains(b.getKind())) return null; // comparisons, && and ||, instanceof: booleans
      String t = temp();
      assign(t, l, "derive");
      assign(t, r, "derive");
      return t;
    }

    private String unary(TreePath path, UnaryTree u) {
      String v = value(path, u.getExpression());
      return switch (u.getKind()) {
        case PREFIX_INCREMENT, PREFIX_DECREMENT, POSTFIX_INCREMENT, POSTFIX_DECREMENT -> v;
        case LOGICAL_COMPLEMENT -> null;
        default -> {
          String t = temp();
          assign(t, v, "derive");
          yield t;
        }
      };
    }

    private String lambda(TreePath path, LambdaExpressionTree l) {
      String id = idOf(l);
      String v = alloc(l, "function", null, id);
      if (id == null) return v;
      Body b = new Body(id, this);
      TreePath lp = new TreePath(path, l);
      b.formals(l.getParameters());
      Tree body = l.getBody();
      if (body instanceof StatementTree) b.statement(new TreePath(lp, body));
      else b.assign(b.ret(), b.value(lp, body));
      return v;
    }

    /**
     * A method reference is the method as a value. A bound receiver (`obj::m`) is
     * not modelled — the qualifier is evaluated, and nothing carries it to the
     * method's `this`.
     */
    private String methodReference(TreePath path, MemberReferenceTree r) {
      Element e = trees.getElement(path);
      Tree qualifier = r.getQualifierExpression();
      if (qualifier != null && !(trees.getElement(new TreePath(path, qualifier)) instanceof TypeElement)) value(path, qualifier);
      String target = e == null || n.unresolved(e) ? null : n.idOf(e);
      return alloc(r, "function", null, target);
    }

    private String switchExpression(TreePath path, SwitchExpressionTree sw) {
      String selector = value(path, sw.getExpression());
      String t = temp();
      yields.push(t);
      TreePath sp = new TreePath(path, sw);
      for (CaseTree c : sw.getCases()) {
        TreePath cp = new TreePath(sp, c);
        bindLabels(cp, c, selector);
        Tree body = c.getBody();
        if (body instanceof ExpressionTree) assign(t, value(cp, body));
        else if (body != null) statement(new TreePath(cp, body));
        else for (StatementTree st : c.getStatements()) statement(new TreePath(cp, st));
      }
      yields.pop();
      return t;
    }

    // ── calls ────────────────────────────────────────────────────────────────

    private String call(TreePath path, MethodInvocationTree t) {
      long from = start(t);
      if (from < 0) return null;
      int cs = x.callSiteId(s, from, pos.getEndPosition(cu, t));
      Element e = trees.getElement(path);
      ExecutableElement m = e instanceof ExecutableElement ex && !n.unresolved(ex) ? ex : null;
      Tree select = t.getMethodSelect();
      String recv = null;
      String field = null;

      if (select instanceof MemberSelectTree ms) {
        field = ms.getIdentifier().toString();
        Element q = trees.getElement(new TreePath(path, ms.getExpression()));
        boolean type = q instanceof TypeElement || q != null && q.getKind() == ElementKind.PACKAGE;
        if (!type) recv = value(path, ms.getExpression());
      } else if (select instanceof IdentifierTree id) {
        field = id.getName().toString();
        // `this(…)` and `super(…)` construct the object being constructed; an
        // unqualified instance call runs on it too.
        if (field.equals("this") || field.equals("super") || m != null && !m.getModifiers().contains(Modifier.STATIC)) recv = thisVar;
      }
      if (recv != null) {
        x.em.emit("receiver", Main.row("call_site", cs, "var", recv));
        // A functional interface's own method is what a lambda runs: the value
        // the receiver holds is the callee, and points-to resolves it.
        TypeElement owner = m == null ? null : Names.enclosingType(m);
        if (m != null && m.getModifiers().contains(Modifier.ABSTRACT) && owner != null
            && owner.getKind() == ElementKind.INTERFACE && n.functional(owner.asType())) {
          x.em.emit("callee_var", Main.row("call_site", cs, "var", recv));
        }
      }
      actuals(path, cs, t.getArguments());

      if (m != null && m.getReturnType().getKind() == TypeKind.VOID) return null;
      String r = temp();
      x.em.emit("actual_ret", Main.row("call_site", cs, "var", r));
      // A record accessor the compiler wrote reads its component.
      if (recv != null && m != null && component(m) != null) loadInto(r, recv, field);
      return r;
    }

    private String newClass(TreePath path, NewClassTree t) {
      long from = start(t);
      int cs = from < 0 ? -1 : x.callSiteId(s, from, pos.getEndPosition(cu, t));
      value(path, t.getEnclosingExpression());
      String instantiated = null;
      if (t.getClassBody() != null) {
        instantiated = idOf(t.getClassBody());
        type(new TreePath(path, t.getClassBody()));
      } else {
        Element e = trees.getElement(path);
        TypeElement declared = e == null ? null : Names.enclosingType(e);
        if (declared != null && !n.unresolved(declared)) instantiated = n.idOf(declared);
      }
      String o = alloc(t, "instance", instantiated, null);
      if (cs >= 0) x.em.emit("receiver", Main.row("call_site", cs, "var", o));
      List<String> args = actuals(path, cs, t.getArguments());
      // A record whose canonical constructor the compiler wrote: each argument is its component.
      if (trees.getElement(path) instanceof ExecutableElement ctor && !declared(ctor)) {
        TypeElement rec = Names.enclosingType(ctor);
        if (rec != null && rec.getKind() == ElementKind.RECORD) {
          List<? extends RecordComponentElement> components = rec.getRecordComponents();
          for (int i = 0; i < components.size() && i < args.size(); i++) {
            store(o, components.get(i).getSimpleName().toString(), args.get(i));
          }
        }
      }
      return o;
    }

    private List<String> actuals(TreePath path, int cs, List<? extends ExpressionTree> args) {
      List<String> vars = new java.util.ArrayList<>();
      for (int i = 0; i < args.size(); i++) {
        String v = value(path, args.get(i));
        vars.add(v);
        if (v != null && cs >= 0) x.em.emit("actual", Main.row("call_site", cs, "index", i, "var", v));
      }
      return vars;
    }

    /** The record component a compiler-written accessor reads, or null. */
    private RecordComponentElement component(ExecutableElement m) {
      if (!m.getParameters().isEmpty() || declared(m)) return null;
      TypeElement owner = Names.enclosingType(m);
      if (owner == null || owner.getKind() != ElementKind.RECORD) return null;
      for (RecordComponentElement c : owner.getRecordComponents()) {
        if (c.getSimpleName().equals(m.getSimpleName())) return c;
      }
      return null;
    }

    private String newArray(TreePath path, NewArrayTree t) {
      String a = alloc(t, "array", null, null);
      for (ExpressionTree d : t.getDimensions()) value(path, d);
      List<? extends ExpressionTree> init = t.getInitializers();
      if (init != null) {
        for (ExpressionTree el : init) store(a, "[]", value(path, el));
      }
      return a;
    }

    // ── assignment targets ───────────────────────────────────────────────────

    /** `target = value`, for any assignable target: a name, a field, an array element. */
    void assignTo(TreePath path, String v) {
      Tree t = path.getLeaf();
      if (t instanceof ParenthesizedTree p) {
        assignTo(new TreePath(path, p.getExpression()), v);
        return;
      }
      if (t instanceof ArrayAccessTree aa) {
        String base = value(path, aa.getExpression());
        value(path, aa.getIndex());
        store(base, "[]", v);
        return;
      }
      if (!(t instanceof IdentifierTree) && !(t instanceof MemberSelectTree)) return;
      Element el = trees.getElement(path);
      if (el == null) return;
      String simple = t instanceof IdentifierTree id ? id.getName().toString() : ((MemberSelectTree) t).getIdentifier().toString();
      switch (el.getKind()) {
        case LOCAL_VARIABLE, PARAMETER, EXCEPTION_PARAMETER, RESOURCE_VARIABLE, BINDING_VARIABLE -> {
          String id = n.projectId(el);
          if (id != null) assign(declare(id, ownerOf(id), varKind(el)), v);
        }
        case FIELD, ENUM_CONSTANT, RECORD_COMPONENT -> store(baseOf(path, el), simple, v);
        default -> {}
      }
    }

    // ── statements ───────────────────────────────────────────────────────────

    void statement(TreePath path, Tree child) {
      if (child != null) statement(new TreePath(path, child));
    }

    void statement(TreePath path) {
      Tree t = path.getLeaf();
      if (t instanceof BlockTree b) {
        for (StatementTree st : b.getStatements()) statement(path, st);
      } else if (t instanceof VariableTree) {
        local(path);
      } else if (t instanceof ExpressionStatementTree es) {
        value(path, es.getExpression());
      } else if (t instanceof ReturnTree r) {
        if (r.getExpression() != null) assign(ret(), value(path, r.getExpression()));
      } else if (t instanceof ThrowTree th) {
        declare(THROWN, fn, "thrown");
        assign(THROWN, value(path, th.getExpression()));
      } else if (t instanceof IfTree i) {
        value(path, i.getCondition());
        statement(path, i.getThenStatement());
        statement(path, i.getElseStatement());
      } else if (t instanceof WhileLoopTree w) {
        value(path, w.getCondition());
        statement(path, w.getStatement());
      } else if (t instanceof DoWhileLoopTree d) {
        statement(path, d.getStatement());
        value(path, d.getCondition());
      } else if (t instanceof ForLoopTree f) {
        for (StatementTree init : f.getInitializer()) statement(path, init);
        value(path, f.getCondition());
        for (ExpressionStatementTree u : f.getUpdate()) statement(path, u);
        statement(path, f.getStatement());
      } else if (t instanceof EnhancedForLoopTree f) {
        String over = value(path, f.getExpression());
        String element = over == null ? null : load(over, "[]");
        String id = idOf(f.getVariable());
        if (id != null) assign(declare(id, fn, "local"), element);
        statement(path, f.getStatement());
      } else if (t instanceof SwitchTree sw) {
        switchStatement(path, sw);
      } else if (t instanceof SynchronizedTree sy) {
        value(path, sy.getExpression());
        statement(path, sy.getBlock());
      } else if (t instanceof TryTree tr) {
        tryStatement(path, tr);
      } else if (t instanceof LabeledStatementTree l) {
        statement(path, l.getStatement());
      } else if (t instanceof YieldTree y) {
        String target = yields.peek();
        if (target != null) assign(target, value(path, y.getValue()));
      } else if (t instanceof AssertTree a) {
        value(path, a.getCondition());
        value(path, a.getDetail());
      } else if (t instanceof ClassTree) {
        type(path);
      }
    }

    /** A declaration binds its variable to its initializer. */
    private void local(TreePath path) {
      VariableTree v = (VariableTree) path.getLeaf();
      String id = idOf(v);
      if (id == null) return;
      declare(id, fn, varKindOf(path));
      if (v.getInitializer() != null) assign(id, value(path, v.getInitializer()));
    }

    private void switchStatement(TreePath path, SwitchTree sw) {
      String selector = value(path, sw.getExpression());
      for (CaseTree c : sw.getCases()) {
        TreePath cp = new TreePath(path, c);
        bindLabels(cp, c, selector);
        Tree body = c.getBody();
        if (body instanceof ExpressionTree) value(cp, body);
        else if (body != null) statement(new TreePath(cp, body));
        else for (StatementTree st : c.getStatements()) statement(cp, st);
      }
    }

    private void tryStatement(TreePath path, TryTree t) {
      for (Tree r : t.getResources()) {
        if (r instanceof VariableTree v) local(new TreePath(path, v));
        else value(path, r);
      }
      statement(path, t.getBlock());
      for (CatchTree c : t.getCatches()) {
        TreePath cp = new TreePath(path, c);
        String id = idOf(c.getParameter());
        if (id != null) {
          declare(THROWN, fn, "thrown");
          assign(declare(id, fn, "catch"), THROWN);
        }
        statement(cp, c.getBlock());
      }
      statement(path, t.getFinallyBlock());
    }

    // ── patterns ─────────────────────────────────────────────────────────────

    /** A case's patterns bind from the selector; its constants and guard are read. */
    private void bindLabels(TreePath path, CaseTree c, String selector) {
      for (Tree label : labelsOf(c)) {
        if (label.getKind().name().equals("PATTERN_CASE_LABEL")) {
          Tree pattern = treeThrough("com.sun.source.tree.PatternCaseLabelTree", label, "getPattern");
          if (pattern != null) bindPattern(path, pattern, selector);
        } else if (label instanceof ExpressionTree) {
          value(path, label);
        }
      }
      Tree guard = guardOf(c);
      if (guard != null) value(path, guard);
    }

    /**
     * A type pattern binds the value itself; a record pattern loads each
     * component into its nested pattern. Record patterns are reached reflectively:
     * the frontend is compiled at a release that predates them.
     */
    private void bindPattern(TreePath path, Tree pattern, String from) {
      if (pattern instanceof BindingPatternTree bp) {
        String id = idOf(bp.getVariable());
        if (id != null) assign(declare(id, fn, "local"), from);
        return;
      }
      Tree deconstructor = null;
      List<? extends Tree> nested = null;
      for (String iface : DECONSTRUCTION) {
        deconstructor = treeThrough(iface, pattern, "getDeconstructor");
        nested = treesThrough(iface, pattern, "getNestedPatterns");
        if (deconstructor != null && nested != null) break;
      }
      if (deconstructor == null || nested == null) return;
      TypeMirror tm = trees.getTypeMirror(new TreePath(path, deconstructor));
      if (tm == null || !(n.types.asElement(tm) instanceof TypeElement rec) || rec.getKind() != ElementKind.RECORD) return;
      List<? extends RecordComponentElement> components = rec.getRecordComponents();
      for (int i = 0; i < nested.size() && i < components.size(); i++) {
        String v = from == null ? null : load(from, components.get(i).getSimpleName().toString());
        bindPattern(path, nested.get(i), v);
      }
    }

    // ── where a variable belongs ─────────────────────────────────────────────

    /** A local declared in a lambda belongs to the lambda; its symbol row says which. */
    private String ownerOf(String id) {
      Map<String, Object> row = x.symbols.get(id);
      Object parent = row == null ? null : row.get("parent");
      return parent instanceof String p ? p : fn;
    }

    private String varKindOf(TreePath path) {
      Element e = trees.getElement(path);
      return e == null ? "local" : varKind(e);
    }
  }

  private static String varKind(Element e) {
    return switch (e.getKind()) {
      case PARAMETER -> "param";
      case EXCEPTION_PARAMETER -> "catch";
      default -> "local";
    };
  }

  /** The deconstruction pattern's interface: named for records when it arrived, renamed since. */
  private static final List<String> DECONSTRUCTION =
      List.of("com.sun.source.tree.DeconstructionPatternTree", "com.sun.source.tree.RecordPatternTree");

  private static final java.util.Set<Tree.Kind> DERIVING = java.util.EnumSet.of(
      Tree.Kind.PLUS, Tree.Kind.MINUS, Tree.Kind.MULTIPLY, Tree.Kind.DIVIDE, Tree.Kind.REMAINDER,
      Tree.Kind.AND, Tree.Kind.OR, Tree.Kind.XOR,
      Tree.Kind.LEFT_SHIFT, Tree.Kind.RIGHT_SHIFT, Tree.Kind.UNSIGNED_RIGHT_SHIFT);

  // ── the JDK's shifting tree API ────────────────────────────────────────────

  /** A `case`'s labels: patterns and constants on a JDK that has them, its expressions before. */
  @SuppressWarnings("unchecked")
  private static List<? extends Tree> labelsOf(CaseTree c) {
    try {
      return (List<? extends Tree>) CaseTree.class.getMethod("getLabels").invoke(c);
    } catch (ReflectiveOperationException e) {
      return c.getExpressions();
    }
  }

  private static Tree guardOf(CaseTree c) {
    try {
      return (Tree) CaseTree.class.getMethod("getGuard").invoke(c);
    } catch (ReflectiveOperationException | ClassCastException e) {
      return null;
    }
  }

  /**
   * A method of a tree interface this frontend is compiled too early to name.
   * The method has to come from the public interface: a tree's own class is not
   * public, so its methods cannot be invoked through it.
   */
  private static Object through(String iface, Object on, String method) {
    try {
      Class<?> type = Class.forName(iface);
      if (!type.isInstance(on)) return null;
      return type.getMethod(method).invoke(on);
    } catch (ReflectiveOperationException | RuntimeException e) {
      return null;
    }
  }

  private static Tree treeThrough(String iface, Object on, String method) {
    return through(iface, on, method) instanceof Tree t ? t : null;
  }

  @SuppressWarnings("unchecked")
  private static List<? extends Tree> treesThrough(String iface, Object on, String method) {
    Object got = through(iface, on, method);
    return got instanceof List<?> l ? (List<? extends Tree>) l : null;
  }

  // ── where ──────────────────────────────────────────────────────────────────

  private String idOf(Tree t) {
    return t == null ? null : x.idByKey.get(Extractor.key(s.abs, start(t), t));
  }

  private long start(Tree t) {
    return pos.getStartPosition(cu, t);
  }

  private long line(long offset) {
    return lines.getLineNumber(Math.max(offset, 0));
  }
}

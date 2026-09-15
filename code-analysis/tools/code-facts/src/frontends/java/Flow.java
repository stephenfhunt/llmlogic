package codefacts;

import com.sun.source.tree.AssignmentTree;
import com.sun.source.tree.BinaryTree;
import com.sun.source.tree.BlockTree;
import com.sun.source.tree.BreakTree;
import com.sun.source.tree.CaseTree;
import com.sun.source.tree.CatchTree;
import com.sun.source.tree.ClassTree;
import com.sun.source.tree.CompilationUnitTree;
import com.sun.source.tree.CompoundAssignmentTree;
import com.sun.source.tree.ConditionalExpressionTree;
import com.sun.source.tree.ContinueTree;
import com.sun.source.tree.DoWhileLoopTree;
import com.sun.source.tree.EmptyStatementTree;
import com.sun.source.tree.EnhancedForLoopTree;
import com.sun.source.tree.ExpressionStatementTree;
import com.sun.source.tree.ExpressionTree;
import com.sun.source.tree.ForLoopTree;
import com.sun.source.tree.IdentifierTree;
import com.sun.source.tree.IfTree;
import com.sun.source.tree.LabeledStatementTree;
import com.sun.source.tree.LambdaExpressionTree;
import com.sun.source.tree.LineMap;
import com.sun.source.tree.MethodInvocationTree;
import com.sun.source.tree.MethodTree;
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
import com.sun.source.tree.UnaryTree;
import com.sun.source.tree.VariableTree;
import com.sun.source.tree.WhileLoopTree;
import com.sun.source.tree.YieldTree;
import com.sun.source.util.DocTrees;
import com.sun.source.util.SourcePositions;
import com.sun.source.util.TreePath;
import com.sun.source.util.TreePathScanner;
import com.sun.source.util.TreeScanner;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.function.Supplier;
import javax.lang.model.element.Element;
import javax.lang.model.element.ExecutableElement;
import javax.lang.model.util.Elements;

/**
 * The flow layer for one file, once its source set is analysed: a
 * statement-level control-flow graph for every method, constructor, lambda and
 * initializer block, with the variables each node defines and uses, the branch
 * points cyclomatic complexity counts, and per-function metrics.
 *
 * <p>The model is the TypeScript layer's, so the library reads either: an entry,
 * exit and throw_exit per function; exceptions over-approximated inside a `try`
 * (any node may throw to its handler); a `finally` entered by every jump that
 * crosses it and re-issuing each. What Java adds:
 *
 * <ul>
 *   <li>catch clauses are tested in order, each a `catch` node: `on_true` into its
 *       body, `on_false` to the next, and past the last the exception goes on;
 *   <li>try-with-resources closes its resources in a `finally` of its own, inside
 *       the statement's catches and finally; `synchronized` releases its monitor
 *       in one;
 *   <li>a colon-form switch falls through; an arrow-form one does not;
 *   <li>a switch expression is lowered before the statement holding it, its
 *       `yield` a break to where the statement continues.
 * </ul>
 */
final class Flow {
  private static final Set<String> NO_IMPLICIT_THROW = Set.of("entry", "exit", "throw_exit", "catch", "finally", "break", "continue");
  private static final Set<String> OWNER_KINDS = Set.of("method", "constructor", "function", "static_block");

  private final Extractor x;
  private final Names n;
  private final Extractor.Source s;
  private final CompilationUnitTree cu;
  private final DocTrees trees;
  private final SourcePositions pos;
  private final LineMap lines;

  Flow(Extractor x, Names n, Extractor.Source s) {
    this.x = x;
    this.n = n;
    this.s = s;
    this.cu = s.cu;
    this.trees = n.trees;
    this.pos = trees.getSourcePositions();
    this.lines = cu.getLineMap();
  }

  void run() {
    new TreePathScanner<Void, Void>() {
      @Override
      public Void visitMethod(MethodTree t, Void v) {
        boolean written = !(trees.getElement(getCurrentPath()) instanceof ExecutableElement m) || n.elements.getOrigin(m) == Elements.Origin.EXPLICIT;
        if (written && t.getBody() != null) function(getCurrentPath(), t.getName().contentEquals("<init>") ? "constructor" : "method", t.getParameters(), t.getBody());
        return super.visitMethod(t, v);
      }

      @Override
      public Void visitLambdaExpression(LambdaExpressionTree t, Void v) {
        function(getCurrentPath(), "lambda", t.getParameters(), t.getBody());
        return super.visitLambdaExpression(t, v);
      }

      @Override
      public Void visitBlock(BlockTree t, Void v) {
        TreePath parent = getCurrentPath().getParentPath();
        if (parent != null && parent.getLeaf() instanceof ClassTree) function(getCurrentPath(), "static_block", List.of(), t);
        return super.visitBlock(t, v);
      }
    }.scan(new TreePath(cu), null);
  }

  private void function(TreePath path, String kind, List<? extends VariableTree> params, Tree body) {
    String id = idOf(path.getLeaf());
    if (id == null) return;
    Builder b = new Builder(id, path);
    b.build(params, body);
    b.flush();
    Metrics m = measure(path.getLeaf(), body);
    Tree t = path.getLeaf();
    Map<String, Object> symbol = x.symbols.get(id);
    long line = symbol != null && symbol.get("line") instanceof Number l ? l.longValue() : line(start(t));
    long endLine = line(Math.max(start(t), pos.getEndPosition(cu, t) - 1));
    x.em.emit("fn", Main.row(
        "id", id, "kind", kind, "file", s.path, "line", line, "end_line", endLine, "loc", endLine - line + 1,
        "statements", m.statements, "params", params.size(), "max_nesting", m.maxNesting, "cyclomatic", 1 + b.decisions,
        "cognitive", m.cognitive, "returns", m.returns, "awaits", 0, "yields", 0, "throws", m.throwsCount,
        "halstead_operators", m.halstead.operators(), "halstead_operands", m.halstead.operands(),
        "halstead_distinct_operators", m.halstead.distinctOperators(), "halstead_distinct_operands", m.halstead.distinctOperands()));
  }

  private record Pending(int from, String kind) {}

  private abstract static class Frame {}

  private static final class Loop extends Frame {
    final List<String> labels;
    final List<Pending> breaks = new ArrayList<>();
    final int continueTarget;

    Loop(List<String> labels, int continueTarget) {
      this.labels = labels;
      this.continueTarget = continueTarget;
    }
  }

  /** A switch or a labelled statement, which `break` leaves; a switch expression, which `yield` leaves. */
  private static final class Breakable extends Frame {
    final String kind;
    final List<String> labels;
    final List<Pending> breaks = new ArrayList<>();

    Breakable(String kind, List<String> labels) {
      this.kind = kind;
      this.labels = labels;
    }
  }

  private static final class Catch extends Frame {
    final int node;

    Catch(int node) {
      this.node = node;
    }
  }

  private static final class Finally extends Frame {
    final int node;
    final LinkedHashMap<String, String[]> routed = new LinkedHashMap<>();

    Finally(int node) {
      this.node = node;
    }
  }

  private final class Builder {
    final String fnId;
    final TreePath fnPath;
    final List<Frame> frames = new ArrayList<>();
    final LinkedHashMap<Integer, Object[]> nodes = new LinkedHashMap<>();
    final List<Object[]> edges = new ArrayList<>();
    final List<Object[]> roots = new ArrayList<>();
    final Map<Integer, Integer> nestingOf = new HashMap<>();
    int nesting;
    int entry;
    int exit;
    int throwExit;
    int decisions;

    Builder(String fnId, TreePath fnPath) {
      this.fnId = fnId;
      this.fnPath = fnPath;
    }

    int node(String kind, Tree tree, boolean implicitThrows) {
      int id = x.nextFlowNode++;
      nodes.put(id, new Object[] {kind, tree});
      nestingOf.put(id, nesting);
      if (implicitThrows && !NO_IMPLICIT_THROW.contains(kind)) implicitThrow(id);
      return id;
    }

    /** Joins pending edges to `to`; `relabel` renames plain fall-through only. */
    void connect(List<Pending> preds, int to, String relabel) {
      for (Pending p : preds) edges.add(new Object[] {p.from(), to, relabel != null && p.kind().equals("next") ? relabel : p.kind()});
    }

    void connect(List<Pending> preds, int to) {
      connect(preds, to, null);
    }

    void evaluates(int node, TreePath path) {
      if (path != null) roots.add(new Object[] {node, path});
    }

    void decision(String kind, Integer node, Tree at) {
      decisions++;
      x.em.emit("decision", Main.row("fn", fnId, "node", node, "kind", kind, "nesting", node == null ? nesting : nestingOf.getOrDefault(node, nesting), "file", s.path, "line", line(start(at))));
    }

    void implicitThrow(int node) {
      for (int i = frames.size() - 1; i >= 0; i--) {
        if (frames.get(i) instanceof Catch || frames.get(i) instanceof Finally) {
          jump(List.of(new Pending(node, "throw")), "throw", null, frames.size());
          return;
        }
      }
    }

    /** Sends `preds` along a jump, resolving it from frame depth `depth` outward. */
    void jump(List<Pending> preds, String kind, String label, int depth) {
      String edge = kind.equals("yield") ? "break" : kind;
      for (int i = depth - 1; i >= 0; i--) {
        Frame f = frames.get(i);
        if (f instanceof Finally fin) {
          connect(preds, fin.node, edge);
          fin.routed.put(kind + ":" + (label == null ? "" : label) + ":" + i, new String[] {kind, label});
          return;
        }
        if (kind.equals("throw") && f instanceof Catch c) {
          connect(preds, c.node, "throw");
          return;
        }
        if (kind.equals("break") && f instanceof Loop l && (label == null || l.labels.contains(label))) {
          for (Pending p : preds) l.breaks.add(new Pending(p.from(), p.kind().equals("next") ? "break" : p.kind()));
          return;
        }
        if (kind.equals("break") && f instanceof Breakable b && !b.kind.equals("yield") && (label == null ? !b.kind.equals("label") : b.labels.contains(label))) {
          for (Pending p : preds) b.breaks.add(new Pending(p.from(), p.kind().equals("next") ? "break" : p.kind()));
          return;
        }
        if (kind.equals("yield") && f instanceof Breakable b && b.kind.equals("yield")) {
          for (Pending p : preds) b.breaks.add(new Pending(p.from(), p.kind().equals("next") ? "break" : p.kind()));
          return;
        }
        if (kind.equals("continue") && f instanceof Loop l && (label == null || l.labels.contains(label))) {
          connect(preds, l.continueTarget, "continue");
          return;
        }
      }
      if (kind.equals("return")) connect(preds, exit, "return");
      else if (kind.equals("throw")) connect(preds, throwExit, "throw");
    }

    <T> T nested(Supplier<T> f) {
      nesting++;
      try {
        return f.get();
      } finally {
        nesting--;
      }
    }

    void build(List<? extends VariableTree> params, Tree body) {
      Tree fn = fnPath.getLeaf();
      entry = node("entry", fn, false);
      exit = node("exit", fn, false);
      throwExit = node("throw_exit", fn, false);
      for (VariableTree p : params) evaluates(entry, new TreePath(fnPath, p));
      TreePath bodyPath = new TreePath(fnPath, body);
      if (body instanceof BlockTree block) {
        List<Pending> out = statements(bodyPath, block.getStatements(), List.of(new Pending(entry, "next")));
        connect(out, exit, "next");
      } else {
        // An expression lambda evaluates its body and returns it.
        List<Pending> preds = lowerSwitches(bodyPath, List.of(new Pending(entry, "next")));
        int r = node("return", body, true);
        evaluates(r, bodyPath);
        connect(preds, r);
        jump(List.of(new Pending(r, "return")), "return", null, frames.size());
      }
    }

    List<Pending> statements(TreePath parent, List<? extends StatementTree> list, List<Pending> preds) {
      List<Pending> cur = preds;
      for (StatementTree st : list) cur = statement(new TreePath(parent, st), cur, List.of());
      return cur;
    }

    List<Pending> statement(TreePath path, List<Pending> preds, List<String> labels) {
      Tree st = path.getLeaf();
      if (st instanceof EmptyStatementTree || st instanceof ClassTree) return preds;
      if (st instanceof BlockTree b) {
        return labels.isEmpty() ? statements(path, b.getStatements(), preds) : labelled(labels, () -> statements(path, b.getStatements(), preds));
      }
      if (st instanceof LabeledStatementTree l) {
        List<String> more = new ArrayList<>(labels);
        more.add(l.getLabel().toString());
        return statement(new TreePath(path, l.getStatement()), preds, more);
      }
      if (st instanceof IfTree i) {
        TreePath condPath = new TreePath(path, i.getCondition());
        List<Pending> in = lowerSwitches(condPath, preds);
        int cond = node("cond", i.getCondition(), true);
        evaluates(cond, condPath);
        decision("if", cond, st);
        connect(in, cond);
        Supplier<List<Pending>> inner = () -> {
          List<Pending> out = new ArrayList<>(nested(() -> statement(new TreePath(path, i.getThenStatement()), List.of(new Pending(cond, "on_true")), List.of())));
          if (i.getElseStatement() != null) out.addAll(nested(() -> statement(new TreePath(path, i.getElseStatement()), List.of(new Pending(cond, "on_false")), List.of())));
          else out.add(new Pending(cond, "on_false"));
          return out;
        };
        return labels.isEmpty() ? inner.get() : labelled(labels, inner);
      }
      if (st instanceof WhileLoopTree w) {
        int head = node("loop_head", w.getCondition(), true);
        evaluates(head, new TreePath(path, w.getCondition()));
        decision("while", head, st);
        connect(preds, head);
        return loopBody(new TreePath(path, w.getStatement()), head, head, labels, List.of(new Pending(head, "on_false")), null);
      }
      if (st instanceof DoWhileLoopTree d) {
        int head = node("loop_head", st, false);
        int cond = node("cond", d.getCondition(), true);
        evaluates(cond, new TreePath(path, d.getCondition()));
        decision("do", cond, st);
        connect(preds, head);
        Loop frame = new Loop(labels, cond);
        frames.add(frame);
        List<Pending> out = nested(() -> statement(new TreePath(path, d.getStatement()), List.of(new Pending(head, "next")), List.of()));
        frames.remove(frames.size() - 1);
        connect(out, cond, "next");
        connect(List.of(new Pending(cond, "back")), head);
        List<Pending> exits = new ArrayList<>(List.of(new Pending(cond, "on_false")));
        exits.addAll(frame.breaks);
        return exits;
      }
      if (st instanceof ForLoopTree f) {
        List<Pending> cur = preds;
        if (!f.getInitializer().isEmpty()) {
          int init = node("stmt", f.getInitializer().get(0), true);
          for (StatementTree s0 : f.getInitializer()) evaluates(init, new TreePath(path, s0));
          connect(cur, init);
          cur = List.of(new Pending(init, "next"));
        }
        int head = node("loop_head", f.getCondition() != null ? f.getCondition() : st, true);
        if (f.getCondition() != null) {
          evaluates(head, new TreePath(path, f.getCondition()));
          decision("for", head, st);
        }
        connect(cur, head);
        Integer incr = null;
        if (!f.getUpdate().isEmpty()) {
          incr = node("stmt", f.getUpdate().get(0), true);
          for (ExpressionStatementTree u : f.getUpdate()) evaluates(incr, new TreePath(path, u));
        }
        List<Pending> exits = f.getCondition() != null ? List.of(new Pending(head, "on_false")) : List.of();
        return loopBody(new TreePath(path, f.getStatement()), head, incr != null ? incr : head, labels, exits, incr);
      }
      if (st instanceof EnhancedForLoopTree f) {
        // The iterable is evaluated once; the head fetches each element and binds it.
        int iterable = node("stmt", f.getExpression(), true);
        evaluates(iterable, new TreePath(path, f.getExpression()));
        connect(preds, iterable);
        int head = node("loop_head", f.getVariable(), true);
        evaluates(head, new TreePath(path, f.getVariable()));
        decision("for_of", head, st);
        connect(List.of(new Pending(iterable, "next")), head);
        return loopBody(new TreePath(path, f.getStatement()), head, head, labels, List.of(new Pending(head, "on_false")), null);
      }
      if (st instanceof SwitchTree sw) {
        TreePath selector = new TreePath(path, sw.getExpression());
        return switchOf(path, selector, sw.getCases(), lowerSwitches(selector, preds), labels, false);
      }
      if (st instanceof TryTree t) return labels.isEmpty() ? tryStatement(path, t, preds) : labelled(labels, () -> tryStatement(path, t, preds));
      if (st instanceof SynchronizedTree sy) return labels.isEmpty() ? synchronizedStatement(path, sy, preds) : labelled(labels, () -> synchronizedStatement(path, sy, preds));
      if (st instanceof ReturnTree r) {
        List<Pending> in = r.getExpression() == null ? preds : lowerSwitches(new TreePath(path, r.getExpression()), preds);
        int node = node("return", st, true);
        if (r.getExpression() != null) evaluates(node, new TreePath(path, r.getExpression()));
        connect(in, node);
        jump(List.of(new Pending(node, "return")), "return", null, frames.size());
        return List.of();
      }
      if (st instanceof ThrowTree t) {
        List<Pending> in = lowerSwitches(new TreePath(path, t.getExpression()), preds);
        int node = node("throw", st, true);
        evaluates(node, new TreePath(path, t.getExpression()));
        connect(in, node);
        jump(List.of(new Pending(node, "throw")), "throw", null, frames.size());
        return List.of();
      }
      if (st instanceof BreakTree || st instanceof ContinueTree) {
        String kind = st instanceof BreakTree ? "break" : "continue";
        javax.lang.model.element.Name label = st instanceof BreakTree b ? b.getLabel() : ((ContinueTree) st).getLabel();
        int node = node(kind, st, true);
        connect(preds, node);
        jump(List.of(new Pending(node, kind)), kind, label == null ? null : label.toString(), frames.size());
        return List.of();
      }
      if (st instanceof YieldTree y) {
        TreePath value = new TreePath(path, y.getValue());
        List<Pending> in = lowerSwitches(value, preds);
        int node = node("break", st, true);
        evaluates(node, value);
        connect(in, node);
        jump(List.of(new Pending(node, "break")), "yield", null, frames.size());
        return List.of();
      }
      // Everything else is one straight-line node, after any switch expression it holds.
      List<Pending> in = lowerSwitches(path, preds);
      int node = node("stmt", st, true);
      evaluates(node, path);
      connect(in, node);
      List<Pending> out = List.of(new Pending(node, "next"));
      return labels.isEmpty() ? out : labelled(labels, () -> out);
    }

    List<Pending> labelled(List<String> labels, Supplier<List<Pending>> inner) {
      Breakable frame = new Breakable("label", labels);
      frames.add(frame);
      List<Pending> out = new ArrayList<>(inner.get());
      frames.remove(frames.size() - 1);
      out.addAll(frame.breaks);
      return out;
    }

    List<Pending> loopBody(TreePath body, int head, int continueTarget, List<String> labels, List<Pending> exits, Integer incr) {
      Loop frame = new Loop(labels, continueTarget);
      frames.add(frame);
      List<Pending> out = nested(() -> statement(body, List.of(new Pending(head, "on_true")), List.of()));
      frames.remove(frames.size() - 1);
      if (incr != null) {
        connect(out, incr, "next");
        connect(List.of(new Pending(incr, "back")), head);
      } else {
        connect(out, head, "back");
      }
      List<Pending> all = new ArrayList<>(exits);
      all.addAll(frame.breaks);
      return all;
    }

    /**
     * A switch: the selector, then each case's test in source order, skipping
     * `default`; failing them all lands on `default` or leaves. A colon case
     * falls into the next; an arrow case, or a switch expression's, leaves.
     */
    List<Pending> switchOf(TreePath path, TreePath selector, List<? extends CaseTree> cases, List<Pending> preds, List<String> labels, boolean expression) {
      int sw = node("switch", selector.getLeaf(), true);
      evaluates(sw, selector);
      connect(preds, sw);
      Map<CaseTree, List<Pending>> entries = new HashMap<>();
      List<Pending> tests = List.of(new Pending(sw, "next"));
      CaseTree deflt = null;
      for (CaseTree c : cases) {
        TreePath casePath = new TreePath(path, c);
        if (isDefault(c)) {
          deflt = c;
          continue;
        }
        int test = node("case_test", c, true);
        for (Tree label : labelsOf(c)) evaluates(test, new TreePath(casePath, label));
        Tree guard = guardOf(c);
        if (guard != null) evaluates(test, new TreePath(casePath, guard));
        decision("case", test, c);
        connect(tests, test);
        tests = List.of(new Pending(test, "on_false"));
        entries.put(c, List.of(new Pending(test, "case")));
      }
      List<Pending> fallOut = new ArrayList<>();
      if (deflt != null) entries.put(deflt, tests.stream().map(p -> new Pending(p.from(), "default")).toList());
      else fallOut.addAll(tests);

      Breakable frame = new Breakable(expression ? "yield" : "switch", labels);
      frames.add(frame);
      List<Pending> fall = nested(() -> {
        List<Pending> f = List.of();
        for (CaseTree c : cases) {
          TreePath casePath = new TreePath(path, c);
          List<Pending> into = new ArrayList<>(entries.getOrDefault(c, List.of()));
          if (c.getCaseKind() == CaseTree.CaseKind.RULE) {
            Tree body = c.getBody();
            List<Pending> out;
            if (body instanceof BlockTree b) out = statements(new TreePath(casePath, b), b.getStatements(), into);
            else if (body instanceof StatementTree bodyStatement) out = statement(new TreePath(casePath, bodyStatement), into, List.of());
            else if (body instanceof ExpressionTree e) {
              TreePath value = new TreePath(casePath, e);
              List<Pending> in = lowerSwitches(value, into);
              int v = node("stmt", e, true);
              evaluates(v, value);
              connect(in, v);
              out = List.of(new Pending(v, "next"));
            } else {
              out = into;
            }
            frame.breaks.addAll(out);
            f = List.of();
          } else {
            into.addAll(f);
            f = statements(casePath, c.getStatements(), into);
          }
        }
        return f;
      });
      frames.remove(frames.size() - 1);
      List<Pending> all = new ArrayList<>(fall);
      all.addAll(fallOut);
      all.addAll(frame.breaks);
      return all;
    }

    /** Lowers the switch expressions `path` evaluates, in source order, before its own node. */
    List<Pending> lowerSwitches(TreePath path, List<Pending> preds) {
      List<TreePath> found = new ArrayList<>();
      new TreePathScanner<Void, Void>() {
        @Override
        public Void visitSwitchExpression(SwitchExpressionTree t, Void v) {
          found.add(getCurrentPath());
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
      }.scan(path, null);
      List<Pending> cur = preds;
      for (TreePath p : found) {
        SwitchExpressionTree se = (SwitchExpressionTree) p.getLeaf();
        TreePath selector = new TreePath(p, se.getExpression());
        cur = switchOf(p, selector, se.getCases(), lowerSwitches(selector, cur), List.of(), true);
      }
      return cur;
    }

    List<Pending> tryStatement(TreePath path, TryTree t, List<Pending> preds) {
      Finally fin = t.getFinallyBlock() != null ? new Finally(node("finally", t.getFinallyBlock(), false)) : null;
      List<Integer> catches = new ArrayList<>();
      for (CatchTree c : t.getCatches()) {
        int node = node("catch", c, false);
        evaluates(node, new TreePath(new TreePath(path, c), c.getParameter()));
        decision("catch", node, c);
        catches.add(node);
      }
      if (fin != null) frames.add(fin);
      if (!catches.isEmpty()) frames.add(new Catch(catches.get(0)));
      List<Pending> tryOut = nested(() -> t.getResources().isEmpty()
          ? statements(new TreePath(path, t.getBlock()), t.getBlock().getStatements(), preds)
          : withResources(path, t, preds));
      if (!catches.isEmpty()) frames.remove(frames.size() - 1);
      List<Pending> out = new ArrayList<>(tryOut);
      for (int i = 0; i < catches.size(); i++) {
        CatchTree c = t.getCatches().get(i);
        int node = catches.get(i);
        TreePath catchPath = new TreePath(path, c);
        out.addAll(nested(() -> statements(new TreePath(catchPath, c.getBlock()), c.getBlock().getStatements(), List.of(new Pending(node, "on_true")))));
        if (i + 1 < catches.size()) connect(List.of(new Pending(node, "on_false")), catches.get(i + 1));
        else jump(List.of(new Pending(node, "on_false")), "throw", null, frames.size());
      }
      if (fin == null) return out;
      frames.remove(frames.size() - 1);
      connect(out, fin.node, "next");
      List<Pending> finOut = nested(() -> statements(new TreePath(path, t.getFinallyBlock()), t.getFinallyBlock().getStatements(), List.of(new Pending(fin.node, "next"))));
      int depth = frames.size();
      for (String[] r : fin.routed.values()) jump(finOut, r[0], r[1], depth);
      return out.isEmpty() ? List.of() : finOut;
    }

    /** Resources are closed in a `finally` of their own: every exit from the body passes it. */
    List<Pending> withResources(TreePath path, TryTree t, List<Pending> preds) {
      boolean many = t.getResources().size() > 1;
      Finally close = new Finally(node("finally", t.getResources().get(0), false));
      // A later resource's initializer can throw into an earlier one's close.
      if (many) frames.add(close);
      int init = node("stmt", t.getResources().get(0), true);
      for (Tree r : t.getResources()) evaluates(init, new TreePath(path, r));
      connect(preds, init);
      if (!many) frames.add(close);
      List<Pending> bodyOut = statements(new TreePath(path, t.getBlock()), t.getBlock().getStatements(), List.of(new Pending(init, "next")));
      frames.remove(frames.size() - 1);
      connect(bodyOut, close.node, "next");
      List<Pending> closed = List.of(new Pending(close.node, "next"));
      int depth = frames.size();
      for (String[] r : close.routed.values()) jump(closed, r[0], r[1], depth);
      return bodyOut.isEmpty() ? List.of() : closed;
    }

    /** The monitor is released in an implicit `finally`. */
    List<Pending> synchronizedStatement(TreePath path, SynchronizedTree sy, List<Pending> preds) {
      TreePath lock = new TreePath(path, sy.getExpression());
      List<Pending> in = lowerSwitches(lock, preds);
      int enter = node("stmt", sy.getExpression(), true);
      evaluates(enter, lock);
      connect(in, enter);
      Finally release = new Finally(node("finally", sy.getBlock(), false));
      frames.add(release);
      List<Pending> bodyOut = nested(() -> statements(new TreePath(path, sy.getBlock()), sy.getBlock().getStatements(), List.of(new Pending(enter, "next"))));
      frames.remove(frames.size() - 1);
      connect(bodyOut, release.node, "next");
      List<Pending> released = List.of(new Pending(release.node, "next"));
      int depth = frames.size();
      for (String[] r : release.routed.values()) jump(released, r[0], r[1], depth);
      return bodyOut.isEmpty() ? List.of() : released;
    }

    /** Emits nodes, edges, and everything each node's expressions do. */
    void flush() {
      for (Map.Entry<Integer, Object[]> e : nodes.entrySet()) {
        String kind = (String) e.getValue()[0];
        Tree tree = (Tree) e.getValue()[1];
        boolean atEnd = kind.equals("exit") || kind.equals("throw_exit");
        long p = atEnd ? pos.getEndPosition(cu, tree) - 1 : start(tree);
        p = Math.max(p, 0);
        x.em.emit("flow_node", Main.row("id", e.getKey(), "fn", fnId, "kind", kind, "file", s.path, "line", lines.getLineNumber(p), "col", lines.getColumnNumber(p)));
      }
      for (Object[] edge : edges) x.em.emit("flow_edge", Main.row("from", edge[0], "to", edge[1], "kind", edge[2]));
      for (Object[] root : roots) walk((Integer) root[0], (TreePath) root[1]);
    }

    private void walk(int node, TreePath root) {
      Set<String> seen = new HashSet<>();
      new TreePathScanner<Void, Void>() {
        @Override
        public Void visitLambdaExpression(LambdaExpressionTree t, Void v) {
          String id = idOf(t);
          if (id != null && id != fnId) x.em.emit("closure", Main.row("node", node, "fn", id));
          return null;
        }

        @Override
        public Void visitClass(ClassTree t, Void v) {
          return null;
        }

        @Override
        public Void visitSwitchExpression(SwitchExpressionTree t, Void v) {
          return null; // lowered: its selector and cases have nodes of their own
        }

        @Override
        public Void visitMethodInvocation(MethodInvocationTree t, Void v) {
          callAt(node, t);
          return super.visitMethodInvocation(t, v);
        }

        @Override
        public Void visitNewClass(NewClassTree t, Void v) {
          callAt(node, t);
          return super.visitNewClass(t, v);
        }

        @Override
        public Void visitConditionalExpression(ConditionalExpressionTree t, Void v) {
          decision("conditional", node, t);
          return super.visitConditionalExpression(t, v);
        }

        @Override
        public Void visitBinary(BinaryTree t, Void v) {
          if (t.getKind() == Tree.Kind.CONDITIONAL_AND) decision("and", node, t);
          if (t.getKind() == Tree.Kind.CONDITIONAL_OR) decision("or", node, t);
          return super.visitBinary(t, v);
        }

        @Override
        public Void visitVariable(VariableTree t, Void v) {
          // A declaration defines its variable when it has a value: an initializer,
          // or the construct binding it (a parameter, a loop variable, a catch, a resource, a pattern).
          String id = idOf(t);
          Tree parent = getCurrentPath().getParentPath() == null ? null : getCurrentPath().getParentPath().getLeaf();
          boolean bound = t.getInitializer() != null || !(parent instanceof BlockTree) && !(parent instanceof ClassTree);
          if (id != null && bound && seen.add("def " + id)) x.em.emit("def", Main.row("node", node, "var", id));
          return super.visitVariable(t, v);
        }

        @Override
        public Void visitIdentifier(IdentifierTree t, Void v) {
          access(node, getCurrentPath(), seen);
          return null;
        }
      }.scan(root, null);
    }

    private void callAt(int node, Tree call) {
      long start = start(call);
      if (start < 0) return;
      x.em.emit("call_at", Main.row("node", node, "call_site", x.callSiteId(s, start, pos.getEndPosition(cu, call))));
    }

    private void access(int node, TreePath path, Set<String> seen) {
      Element e = trees.getElement(path);
      if (e == null) return;
      switch (e.getKind()) {
        case LOCAL_VARIABLE, PARAMETER, EXCEPTION_PARAMETER, RESOURCE_VARIABLE, BINDING_VARIABLE -> {}
        default -> {
          return;
        }
      }
      String var = n.projectId(e);
      if (var == null) return;
      TreePath p = path;
      while (p.getParentPath() != null && p.getParentPath().getLeaf() instanceof ParenthesizedTree) p = p.getParentPath();
      Tree child = p.getLeaf();
      Tree parent = p.getParentPath() == null ? null : p.getParentPath().getLeaf();
      String access = "use";
      if (parent instanceof AssignmentTree a && a.getVariable() == child) access = "def";
      else if (parent instanceof CompoundAssignmentTree a && a.getVariable() == child) access = "both";
      else if (parent instanceof UnaryTree u && switch (u.getKind()) {
        case PREFIX_INCREMENT, PREFIX_DECREMENT, POSTFIX_INCREMENT, POSTFIX_DECREMENT -> true;
        default -> false;
      }) access = "both";
      if (!access.equals("use") && seen.add("def " + var)) x.em.emit("def", Main.row("node", node, "var", var));
      if (!access.equals("def") && seen.add("use " + var)) x.em.emit("use", Main.row("node", node, "var", var));
      // A variable declared in an enclosing function is captured by this one.
      Map<String, Object> row = x.symbols.get(var);
      Object owner = row == null ? null : row.get("parent");
      if (owner instanceof String o && !o.equals(fnId) && x.symbols.get(o) != null && OWNER_KINDS.contains(String.valueOf(x.symbols.get(o).get("kind")))) {
        if (seen.add("captures " + var)) x.em.emit("captures", Main.row("fn", fnId, "var", var));
      }
    }
  }

  /** A default case: no labels, or a `default` label among them. */
  private static boolean isDefault(CaseTree c) {
    List<? extends Tree> labels = labelsOf(c);
    if (labels.isEmpty()) return true;
    for (Tree l : labels) if (l.getKind().name().equals("DEFAULT_CASE_LABEL")) return true;
    return false;
  }

  /** A case's labels: patterns and constants on a JDK that has them, its expressions before. */
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
    } catch (ReflectiveOperationException e) {
      return null;
    }
  }

  // ── metrics ────────────────────────────────────────────────────────────────

  private static final class Metrics {
    int statements;
    int maxNesting;
    int cognitive;
    int returns;
    int throwsCount;
    Text.Halstead halstead;
  }

  private Metrics measure(Tree fn, Tree body) {
    Metrics m = new Metrics();
    List<int[]> skipped = new ArrayList<>();
    new TreeScanner<Void, int[]>() {
      @Override
      public Void scan(Tree t, int[] ctx) {
        if (t == null) return null;
        if (t != fn && t != body && (t instanceof LambdaExpressionTree || t instanceof ClassTree)) {
          long a = start(t);
          long b = pos.getEndPosition(cu, t);
          if (a >= 0 && b > a) skipped.add(new int[] {(int) a, (int) b});
          return null;
        }
        int depth = ctx[0];
        int cog = ctx[1];
        if (t instanceof StatementTree && !(t instanceof BlockTree)) m.statements++;
        if (t instanceof ReturnTree) m.returns++;
        if (t instanceof ThrowTree) m.throwsCount++;
        if (t instanceof IfTree i) {
          boolean elseIf = ctx.length > 2 && ctx[2] == 1;
          int d = elseIf ? depth : depth + 1;
          m.maxNesting = Math.max(m.maxNesting, d);
          m.cognitive += elseIf ? 1 : 1 + cog;
          if (i.getElseStatement() != null && !(i.getElseStatement() instanceof IfTree)) m.cognitive += 1;
          scan(i.getCondition(), new int[] {d, cog});
          scan(i.getThenStatement(), new int[] {d, cog + 1});
          if (i.getElseStatement() instanceof IfTree) scan(i.getElseStatement(), new int[] {depth, cog, 1});
          else scan(i.getElseStatement(), new int[] {d, cog + 1});
          return null;
        }
        boolean structure = t instanceof WhileLoopTree || t instanceof DoWhileLoopTree || t instanceof ForLoopTree || t instanceof EnhancedForLoopTree
            || t instanceof SwitchTree || t instanceof SwitchExpressionTree || t instanceof CatchTree || t instanceof ConditionalExpressionTree;
        if (structure) {
          m.maxNesting = Math.max(m.maxNesting, depth + 1);
          m.cognitive += 1 + cog;
          return super.scan(t, new int[] {depth + 1, cog + 1});
        }
        if (t instanceof BreakTree b && b.getLabel() != null || t instanceof ContinueTree c && c.getLabel() != null) m.cognitive += 1;
        if (t instanceof BinaryTree bin && (bin.getKind() == Tree.Kind.CONDITIONAL_AND || bin.getKind() == Tree.Kind.CONDITIONAL_OR)) {
          // A run of one operator counts once.
          if (!(bin.getLeftOperand() instanceof BinaryTree l && l.getKind() == bin.getKind())) m.cognitive += 1;
        }
        return super.scan(t, new int[] {depth, cog});
      }
    }.scan(body, new int[] {0, 0});
    long a = start(body);
    long b = pos.getEndPosition(cu, body);
    m.halstead = a < 0 || b <= a ? new Text.Halstead(0, 0, 0, 0) : Text.halstead(s.text, (int) a, (int) b, skipped);
    return m;
  }

  // ── positions ──────────────────────────────────────────────────────────────

  private String idOf(Tree t) {
    return x.idByKey.get(Extractor.key(s.abs, start(t), t));
  }

  private long start(Tree t) {
    return pos.getStartPosition(cu, t);
  }

  private long line(long offset) {
    return lines.getLineNumber(Math.max(offset, 0));
  }
}

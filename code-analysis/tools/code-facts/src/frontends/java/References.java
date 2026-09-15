package codefacts;

import com.sun.source.doctree.DocCommentTree;
import com.sun.source.doctree.ReferenceTree;
import com.sun.source.tree.AnnotationTree;
import com.sun.source.tree.ClassTree;
import com.sun.source.tree.CompilationUnitTree;
import com.sun.source.tree.IdentifierTree;
import com.sun.source.tree.ImportTree;
import com.sun.source.tree.LineMap;
import com.sun.source.tree.MemberReferenceTree;
import com.sun.source.tree.MemberSelectTree;
import com.sun.source.tree.MethodTree;
import com.sun.source.tree.ModifiersTree;
import com.sun.source.tree.PackageTree;
import com.sun.source.tree.Tree;
import com.sun.source.tree.VariableTree;
import com.sun.source.util.DocTreePath;
import com.sun.source.util.DocTreePathScanner;
import com.sun.source.util.DocTrees;
import com.sun.source.util.SourcePositions;
import com.sun.source.util.TreePath;
import com.sun.source.util.TreePathScanner;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.Deque;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.TreeMap;
import javax.lang.model.element.Element;
import javax.lang.model.element.ElementKind;
import javax.lang.model.element.Modifier;
import javax.lang.model.element.PackageElement;
import javax.lang.model.element.TypeElement;

/**
 * One file's resolved names, once its source set is analysed: `imports` — a row
 * per file each import statement is used to reach, and an `implicit` row per
 * file reached with no import at all — `import_name`, and `decorator`.
 */
final class References {
  private final Extractor x;
  private final Names n;
  private final Extractor.Source s;
  private final CompilationUnitTree cu;
  private final DocTrees docTrees;
  private final SourcePositions pos;
  private final LineMap lines;
  private final List<? extends ImportTree> imports;

  /** A use of a declaration outside this file: where it is declared, and the import it came through. */
  private record Ref(Extractor.Source target, long line, long offset, String name, boolean runtime, ImportTree via) {}

  private final List<Ref> refs = new ArrayList<>();
  /** What each import statement names: a type, or for `*` the package or type whose members it imports. */
  private final Map<ImportTree, Element> resolved = new HashMap<>();
  private final Deque<TypeElement> enclosing = new ArrayDeque<>();

  References(Extractor x, Names n, Extractor.Source s) {
    this.x = x;
    this.n = n;
    this.s = s;
    this.cu = s.cu;
    this.docTrees = n.trees;
    this.pos = docTrees.getSourcePositions();
    this.lines = cu.getLineMap();
    this.imports = cu.getImports();
  }

  void run() {
    TreePath unit = new TreePath(cu);
    for (ImportTree it : imports) {
      if (!(it.getQualifiedIdentifier() instanceof MemberSelectTree q)) continue;
      TreePath qualid = new TreePath(new TreePath(unit, it), q);
      Element e = it.isStatic() || star(it) ? docTrees.getElement(new TreePath(qualid, q.getExpression())) : docTrees.getElement(qualid);
      if (e != null && !n.unresolved(e)) resolved.put(it, e);
    }
    new Scanner().scan(unit, null);
    emitImports();
  }

  // ── uses ───────────────────────────────────────────────────────────────────

  private final class Scanner extends TreePathScanner<Void, Void> {
    /** Inside the qualifier of an inlined constant, which the compiled class never names. */
    private boolean constantQualifier;

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
      decorators(getCurrentPath(), t.getModifiers());
      docReferences(getCurrentPath());
      Element e = docTrees.getElement(getCurrentPath());
      TypeElement type = e instanceof TypeElement te ? te : null;
      if (type != null) enclosing.push(type);
      try {
        return super.visitClass(t, v);
      } finally {
        if (type != null) enclosing.pop();
      }
    }

    @Override
    public Void visitMethod(MethodTree t, Void v) {
      decorators(getCurrentPath(), t.getModifiers());
      docReferences(getCurrentPath());
      return super.visitMethod(t, v);
    }

    @Override
    public Void visitVariable(VariableTree t, Void v) {
      decorators(getCurrentPath(), t.getModifiers());
      docReferences(getCurrentPath());
      return super.visitVariable(t, v);
    }

    @Override
    public Void visitIdentifier(IdentifierTree t, Void v) {
      use(getCurrentPath(), t.getName().toString(), constantQualifier);
      return null;
    }

    @Override
    public Void visitMemberSelect(MemberSelectTree t, Void v) {
      Element e = use(getCurrentPath(), t.getIdentifier().toString(), constantQualifier);
      boolean saved = constantQualifier;
      if (Names.constant(e)) constantQualifier = true;
      try {
        return super.visitMemberSelect(t, v);
      } finally {
        constantQualifier = saved;
      }
    }

    @Override
    public Void visitMemberReference(MemberReferenceTree t, Void v) {
      use(getCurrentPath(), t.getName().toString(), constantQualifier);
      return super.visitMemberReference(t, v);
    }
  }

  /** Records a use of what {@code path} names; returns that element. */
  private Element use(TreePath path, String name, boolean inConstantQualifier) {
    Element e = docTrees.getElement(path);
    if (e == null || n.unresolved(e) || Names.local(e)) return e;
    TypeElement top = Names.topLevel(e);
    if (top == null) return e;
    Tree leaf = path.getLeaf();
    ImportTree via = leaf instanceof IdentifierTree ? via(e, name) : null;
    // Where the name itself is written: a qualified name's last segment ends its tree.
    long offset = leaf instanceof IdentifierTree ? pos.getStartPosition(cu, leaf) : pos.getEndPosition(cu, leaf) - name.length();
    refs.add(new Ref(n.declaredIn(top), line(offset), offset, name, !inConstantQualifier && !Names.constant(e), via));
    return e;
  }

  /** `{@link …}` and `@see` in a declaration's Javadoc: uses that never run. */
  private void docReferences(TreePath path) {
    DocCommentTree dc = docTrees.getDocCommentTree(path);
    if (dc == null) return;
    new DocTreePathScanner<Void, Void>() {
      @Override
      public Void visitReference(ReferenceTree r, Void v) {
        Element e = docTrees.getElement(getCurrentPath());
        if (e != null && !n.unresolved(e) && !Names.local(e)) {
          TypeElement top = Names.topLevel(e);
          if (top != null) {
            long offset = docTrees.getSourcePositions().getStartPosition(cu, dc, r);
            TypeElement named = e instanceof TypeElement t ? t : Names.enclosingType(e);
            refs.add(new Ref(n.declaredIn(top), line(offset), offset, r.getSignature(), false, named == null ? null : via(named, named.getSimpleName().toString())));
          }
        }
        return null;
      }
    }.scan(new DocTreePath(path, dc), null);
  }

  /** The import statement a simple name was resolved through, if any. */
  private ImportTree via(Element e, String name) {
    if (e instanceof TypeElement type) {
      for (ImportTree it : imports) {
        if (!it.isStatic() && !star(it) && type.equals(resolved.get(it))) return it;
      }
      if (inScope(type)) return null;
      Element owner = type.getEnclosingElement();
      if (owner instanceof PackageElement p && (p.getQualifiedName().contentEquals(s.pkg) || p.getQualifiedName().contentEquals("java.lang"))) return null;
      for (ImportTree it : imports) {
        if (!it.isStatic() && star(it) && owner.equals(resolved.get(it))) return it;
      }
      return null;
    }
    boolean member = e.getKind().isField() || e.getKind() == ElementKind.METHOD;
    if (!member || !e.getModifiers().contains(Modifier.STATIC) || inScope(e)) return null;
    for (ImportTree it : imports) {
      if (it.isStatic() && !star(it) && simpleName(it).equals(name) && resolved.get(it) instanceof TypeElement t && n.membersOf(t).contains(e)) return it;
    }
    for (ImportTree it : imports) {
      if (it.isStatic() && star(it) && resolved.get(it) instanceof TypeElement t && n.membersOf(t).contains(e)) return it;
    }
    return null;
  }

  /** Named without an import: the enclosing types, their members, and what they inherit. */
  private boolean inScope(Element e) {
    for (TypeElement c : enclosing) {
      if (c.equals(e) || n.membersOf(c).contains(e)) return true;
    }
    return false;
  }

  // ── rows ───────────────────────────────────────────────────────────────────

  private record Row(long line, String specifier, String kind, Boolean runtime, String targetFile, String targetPackage, boolean builtin, boolean resolved, String targetDir) {}

  private void emitImports() {
    Map<ImportTree, TreeMap<String, Boolean>> viaFiles = new HashMap<>();
    Map<ImportTree, Boolean> viaOutside = new HashMap<>();
    TreeMap<String, Ref> implicit = new TreeMap<>();
    Map<String, Boolean> implicitRuntime = new HashMap<>();
    for (Ref r : refs) {
      if (r.via() != null) {
        if (r.target() != null) viaFiles.computeIfAbsent(r.via(), k -> new TreeMap<>()).merge(r.target().path, r.runtime(), Boolean::logicalOr);
        else viaOutside.merge(r.via(), r.runtime(), Boolean::logicalOr);
      } else if (r.target() != null && r.target() != s) {
        Ref first = implicit.get(r.target().path);
        if (first == null || r.offset() < first.offset()) implicit.put(r.target().path, r);
        implicitRuntime.merge(r.target().path, r.runtime(), Boolean::logicalOr);
      }
    }

    List<Row> rows = new ArrayList<>();
    Set<String> explicit = new HashSet<>();
    for (ImportTree it : imports) {
      if (!(it.getQualifiedIdentifier() instanceof MemberSelectTree q)) continue;
      long line = line(pos.getStartPosition(cu, it));
      String specifier = source(q).replaceAll("\\s+", "");
      boolean star = star(it);
      String kind = it.isStatic() ? "static_import" : star ? "on_demand" : "static";
      String simple = simpleName(it);
      Element e = resolved.get(it);
      if (e == null) {
        rows.add(new Row(line, specifier, kind, null, null, null, false, false, null));
        name(line, star ? "*" : simple, null);
        continue;
      }
      TreeMap<String, Boolean> files = new TreeMap<>(viaFiles.getOrDefault(it, new TreeMap<>()));
      // What a single-type or static import names is a dependency whether or not it is used.
      if ((!star || it.isStatic()) && e instanceof TypeElement t) {
        Extractor.Source own = n.declaredIn(Names.topLevel(t));
        if (own != null) files.putIfAbsent(own.path, false);
      }
      if (!files.isEmpty()) {
        for (Map.Entry<String, Boolean> f : files.entrySet()) {
          rows.add(new Row(line, specifier, kind, f.getValue(), f.getKey(), null, false, true, Extractor.dirOf(f.getKey())));
          explicit.add(f.getKey());
        }
      } else if (star && projectDir(e) != null) {
        rows.add(new Row(line, specifier, kind, false, null, null, false, true, projectDir(e)));
      } else {
        rows.add(new Row(line, specifier, kind, viaOutside.getOrDefault(it, false), null, n.packageOf(e), n.jdk(e), true, null));
      }

      if (star) {
        name(line, "*", n.idOf(e));
      } else if (!it.isStatic()) {
        name(line, simple, n.idOf(e));
      } else {
        boolean any = false;
        if (e instanceof TypeElement t) {
          for (Element m : n.membersOf(t)) {
            if (m.getModifiers().contains(Modifier.STATIC) && m.getSimpleName().contentEquals(simple)) {
              name(line, simple, n.idOf(m));
              any = true;
            }
          }
        }
        if (!any) name(line, simple, null);
      }
    }
    for (Map.Entry<String, Ref> e : implicit.entrySet()) {
      if (explicit.contains(e.getKey())) continue;
      Ref r = e.getValue();
      rows.add(new Row(r.line(), r.name(), "implicit", implicitRuntime.get(e.getKey()), e.getKey(), null, false, true, Extractor.dirOf(e.getKey())));
    }
    rows.sort(Comparator.comparingLong(Row::line).thenComparing(r -> r.targetFile() == null ? "" : r.targetFile()));
    for (Row r : rows) {
      x.em.emit("imports", Main.row(
          "file", s.path, "line", r.line(), "specifier", r.specifier(), "kind", r.kind(), "runtime", r.runtime(),
          "target_file", r.targetFile(), "target_package", r.targetPackage(), "target_ambient", null,
          "builtin", r.builtin(), "resolved", r.resolved(), "unresolved_package", null, "target_dir", r.targetDir()));
    }
  }

  private void name(long line, String local, String target) {
    x.em.emit("import_name", Main.row("file", s.path, "line", line, "local", local, "imported", local, "target", target, "type_only", false));
  }

  private void decorators(TreePath path, ModifiersTree mods) {
    if (mods == null || mods.getAnnotations().isEmpty()) return;
    String target = x.idByKey.get(s.abs + ":" + pos.getStartPosition(cu, path.getLeaf()));
    if (target == null) return;
    TreePath modsPath = new TreePath(path, mods);
    for (AnnotationTree a : mods.getAnnotations()) {
      TreePath typePath = new TreePath(new TreePath(modsPath, a), a.getAnnotationType());
      Element type = docTrees.getElement(typePath);
      String written = source(a.getAnnotationType()).replaceAll("\\s+", "");
      x.em.emit("decorator", Main.row(
          "target", target, "decorator", type == null || n.unresolved(type) ? null : n.idOf(type), "name", written,
          "line", line(pos.getStartPosition(cu, a)), "text", arguments(a)));
    }
  }

  /** The annotation's arguments as written, between its parentheses. */
  private String arguments(AnnotationTree a) {
    if (a.getArguments().isEmpty()) return null;
    String written = source(a);
    int open = written.indexOf('(', source(a.getAnnotationType()).length());
    int close = written.lastIndexOf(')');
    if (open < 0 || close <= open) return null;
    String args = written.substring(open + 1, close).strip();
    return args.isEmpty() ? null : Text.truncate(args, 200);
  }

  // ── what an element is ─────────────────────────────────────────────────────

  private String projectDir(Element e) {
    if (e instanceof PackageElement p) return x.packageDirs.get(p.getQualifiedName().toString());
    Extractor.Source src = e instanceof TypeElement t ? n.declaredIn(Names.topLevel(t)) : null;
    return src == null ? null : Extractor.dirOf(src.path);
  }

  private static boolean star(ImportTree it) {
    return it.getQualifiedIdentifier() instanceof MemberSelectTree q && q.getIdentifier().contentEquals("*");
  }

  private static String simpleName(ImportTree it) {
    return it.getQualifiedIdentifier() instanceof MemberSelectTree q ? q.getIdentifier().toString() : it.getQualifiedIdentifier().toString();
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

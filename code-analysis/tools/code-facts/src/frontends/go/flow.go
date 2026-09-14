package main

import (
	"go/ast"
	"go/scanner"
	"go/token"
	"go/types"
	"sort"
	"strconv"
)

// The flow layer: a statement-level control-flow graph per function body, and
// one per file for its package-level variable initializers; def/use of
// variables at each node; the branch points that make up cyclomatic
// complexity; per-function metrics; and where goroutines start, calls are
// deferred and channels are used.
//
// The model is the TypeScript layer's, so lib/ reads either language. What Go
// adds:
//
//   - `defer` is a `finally` around the whole body: one node per function where
//     the deferred calls run, entered by the body's end, every return and every
//     panic. It loops back on itself when more than one call may be deferred,
//     and after a panic it may resume at `exit` when a deferred call may
//     `recover`. Inside a function that defers, any node may panic, as inside a
//     `try`; elsewhere only `panic(…)` reaches `throw_exit`.
//   - `goto` and `fallthrough` are nodes and edges of their own; a label some
//     `goto` targets is a node.
//   - `select` evaluates every channel operand on entry, then takes one case:
//     its cases are tested in turn like a switch's, and without `default` the
//     last is taken when none before it is.
//   - a type switch's symbolic variable is defined at the switch.

type pending struct {
	from int
	kind string
}

type flowFrame struct {
	kind           string // loop, switch (also a type switch or select), finally
	labels         []string
	breaks         []pending
	continueTarget int
	node           int
	routed         []string
}

func (f *flowFrame) route(kind string) {
	for _, k := range f.routed {
		if k == kind {
			return
		}
	}
	f.routed = append(f.routed, kind)
}

func (f *flowFrame) wasRouted(kind string) bool {
	for _, k := range f.routed {
		if k == kind {
			return true
		}
	}
	return false
}

type flowNodeRec struct {
	id        int
	kind      string
	pos       token.Pos
	line, col int // set when there is no position to read
}

type flowEdge struct {
	from, to int
	kind     string
}

type flowRoot struct {
	node int
	expr ast.Node
}

var noImplicitThrow = map[string]bool{
	"entry": true, "exit": true, "throw_exit": true, "catch": true, "finally": true,
	"break": true, "continue": true, "goto": true, "fallthrough": true,
}

var flowVarKinds = map[any]bool{"local": true, "parameter": true, "variable": true}

type fnBuilder struct {
	x      *extractor
	s      *source
	info   *types.Info
	parent map[ast.Node]ast.Node
	fnID   string

	frames    []*flowFrame
	nodes     []flowNodeRec
	edges     []flowEdge
	roots     []flowRoot
	nesting   int
	nestingOf map[int]int
	decisions int

	entry, exit, throwExit int
	fin                    *flowFrame
	deferred               []*ast.CallExpr
	results                []*ast.Ident
	gotoTargets            map[string]bool
	labelNode              map[string]int
	gotos                  map[string][]pending
	recorded               map[string]bool
	concurrency            []row
}

func (x *extractor) newFnBuilder(s *source, parent map[ast.Node]ast.Node, fnID string) *fnBuilder {
	return &fnBuilder{
		x: x, s: s, info: s.pkg.TypesInfo, parent: parent, fnID: fnID,
		nestingOf: map[int]int{}, gotoTargets: map[string]bool{}, labelNode: map[string]int{},
		gotos: map[string][]pending{}, recorded: map[string]bool{},
	}
}

func (b *fnBuilder) node(kind string, pos token.Pos, implicit bool) int {
	id := b.x.nextFlowNode
	b.x.nextFlowNode++
	b.nodes = append(b.nodes, flowNodeRec{id: id, kind: kind, pos: pos})
	b.nestingOf[id] = b.nesting
	if implicit && !noImplicitThrow[kind] {
		b.implicitThrow(id)
	}
	return id
}

// connect joins pending edges to `to`. `relabel` renames plain fall-through
// only: a branch edge keeps its kind, or two branches meeting at one node would
// collapse into one edge.
func (b *fnBuilder) connect(preds []pending, to int, relabel string) {
	for _, p := range preds {
		kind := p.kind
		if relabel != "" && kind == "next" {
			kind = relabel
		}
		b.edges = append(b.edges, flowEdge{p.from, to, kind})
	}
}

func (b *fnBuilder) evaluates(node int, exprs ...ast.Node) {
	for _, e := range exprs {
		if e != nil {
			b.roots = append(b.roots, flowRoot{node, e})
		}
	}
}

func (b *fnBuilder) decision(kind string, node int, pos token.Pos, nesting int) {
	b.decisions++
	b.x.em.emit("decision", row{"fn": b.fnID, "node": node, "kind": kind, "nesting": nesting, "file": b.s.path, "line": b.x.line(pos)})
}

func (b *fnBuilder) concurrent(node int, kind string, pos token.Pos) {
	b.concurrency = append(b.concurrency, row{"fn": b.fnID, "node": node, "kind": kind, "file": b.s.path, "line": b.x.line(pos)})
}

func (b *fnBuilder) nested(f func()) {
	b.nesting++
	defer func() { b.nesting-- }()
	f()
}

// implicitThrow: a panic raised at `node`, to the deferred calls if the
// function has any.
func (b *fnBuilder) implicitThrow(node int) {
	for _, f := range b.frames {
		if f.kind == "finally" {
			b.jump([]pending{{node, "throw"}}, "throw", "", len(b.frames))
			return
		}
	}
}

// jump sends `preds` along a jump, resolving it from frame depth `depth` outward.
func (b *fnBuilder) jump(preds []pending, kind, label string, depth int) {
	for i := depth - 1; i >= 0; i-- {
		f := b.frames[i]
		switch {
		case f.kind == "finally":
			b.connect(preds, f.node, kind)
			f.route(kind)
			return
		case kind == "break" && (f.kind == "loop" || f.kind == "switch") && (label == "" || contains(f.labels, label)):
			for _, p := range preds {
				k := p.kind
				if k == "next" {
					k = "break"
				}
				f.breaks = append(f.breaks, pending{p.from, k})
			}
			return
		case kind == "continue" && f.kind == "loop" && (label == "" || contains(f.labels, label)):
			b.connect(preds, f.continueTarget, "continue")
			return
		}
	}
	switch kind {
	case "return":
		b.connect(preds, b.exit, "return")
	case "throw":
		b.connect(preds, b.throwExit, "throw")
	}
}

func contains(xs []string, s string) bool {
	for _, x := range xs {
		if x == s {
			return true
		}
	}
	return false
}

// ── functions ───────────────────────────────────────────────────────────────

func (x *extractor) emitFlow() {
	for _, s := range x.sources {
		parent := parents(s.file)
		toks := fileTokens(s.text)
		x.moduleFlow(s, parent, toks)
		ast.Inspect(s.file, func(n ast.Node) bool {
			switch f := n.(type) {
			case *ast.FuncDecl:
				if f.Body == nil {
					return true
				}
				if id, ok := x.idOf(s.pkg.TypesInfo.Defs[f.Name]); ok {
					kind := "function"
					if f.Recv != nil {
						kind = "method"
					}
					x.fnFlow(s, parent, toks, id, kind, f, f.Type, f.Recv, f.Body)
				}
			case *ast.FuncLit:
				if id, ok := x.idByKey[x.posKey(f.Pos())+":lit"]; ok {
					x.fnFlow(s, parent, toks, id, "function_expression", f, f.Type, nil, f.Body)
				}
			}
			return true
		})
	}
}

func (x *extractor) fnFlow(s *source, parent map[ast.Node]ast.Node, toks []srcToken, id, kind string, fn ast.Node, ftype *ast.FuncType, recv *ast.FieldList, body *ast.BlockStmt) {
	b := x.newFnBuilder(s, parent, id)
	b.entry = b.node("entry", fn.Pos(), false)
	b.exit = b.node("exit", body.Rbrace, false)
	b.throwExit = b.node("throw_exit", body.Rbrace, false)
	b.buildBody(ftype, recv, body)
	b.flush()

	m := b.measure(body)
	h := halstead(x, s, toks, body.Lbrace, body.Rbrace+1, body)
	params := 0
	if ftype.Params != nil {
		for _, f := range ftype.Params.List {
			params += max(1, len(f.Names))
		}
	}
	line, end := x.line(fn.Pos()), x.line(fn.End())
	x.em.emit("fn", row{
		"id": id, "kind": kind, "file": s.path, "line": line, "end_line": end, "loc": end - line + 1,
		"statements": m.statements, "params": params, "max_nesting": m.maxNesting,
		"cyclomatic": 1 + b.decisions, "cognitive": m.cognitive, "returns": m.returns, "awaits": 0, "yields": 0,
		"throws": m.throws, "halstead_operators": h.operators, "halstead_operands": h.operands,
		"halstead_distinct_operators": h.distinctOperators, "halstead_distinct_operands": h.distinctOperands,
	})
}

// moduleFlow: a file's package-level variables and constants, initialized in
// source order (Go orders them by dependency; the graph does not).
func (x *extractor) moduleFlow(s *source, parent map[ast.Node]ast.Node, toks []srcToken) {
	id := moduleID(s)
	b := x.newFnBuilder(s, parent, id)
	loc := lineCount(s.text)
	b.entry = b.node("entry", token.NoPos, false)
	b.nodes[len(b.nodes)-1].line, b.nodes[len(b.nodes)-1].col = 1, 1
	b.exit = b.node("exit", token.NoPos, false)
	b.nodes[len(b.nodes)-1].line, b.nodes[len(b.nodes)-1].col = max(loc, 1), 1
	b.throwExit = b.node("throw_exit", token.NoPos, false)
	b.nodes[len(b.nodes)-1].line, b.nodes[len(b.nodes)-1].col = max(loc, 1), 1
	preds := []pending{{b.entry, "next"}}
	var decls []ast.Node
	for _, d := range s.file.Decls {
		gd, ok := d.(*ast.GenDecl)
		if !ok || (gd.Tok != token.VAR && gd.Tok != token.CONST) {
			continue
		}
		decls = append(decls, gd)
		for _, spec := range gd.Specs {
			n := b.node("stmt", spec.Pos(), true)
			b.evaluates(n, spec)
			b.connect(preds, n, "")
			preds = []pending{{n, "next"}}
		}
	}
	b.connect(preds, b.exit, "next")
	b.flush()

	m := fnMetrics{}
	for _, d := range decls {
		m.add(b.measure(d))
	}
	h := halstead(x, s, toks, token.NoPos, token.NoPos, nil)
	x.em.emit("fn", row{
		"id": id, "kind": "module", "file": s.path, "line": 1, "end_line": max(loc, 1), "loc": max(loc, 1),
		"statements": m.statements, "params": 0, "max_nesting": m.maxNesting,
		"cyclomatic": 1 + b.decisions, "cognitive": m.cognitive, "returns": 0, "awaits": 0, "yields": 0,
		"throws": m.throws, "halstead_operators": h.operators, "halstead_operands": h.operands,
		"halstead_distinct_operators": h.distinctOperators, "halstead_distinct_operands": h.distinctOperands,
	})
}

func (b *fnBuilder) buildBody(ftype *ast.FuncType, recv *ast.FieldList, body *ast.BlockStmt) {
	for _, fl := range []*ast.FieldList{recv, ftype.Params, ftype.Results} {
		if fl == nil {
			continue
		}
		for _, f := range fl.List {
			for _, n := range f.Names {
				b.evaluates(b.entry, n)
			}
		}
	}
	if ftype.Results != nil {
		for _, f := range ftype.Results.List {
			b.results = append(b.results, f.Names...)
		}
	}
	defers, repeated, recovers := b.scan(body)
	if defers > 0 {
		b.fin = &flowFrame{kind: "finally", node: b.node("finally", body.Rbrace, false)}
		b.frames = append(b.frames, b.fin)
	}
	out := b.statements(body.List, []pending{{b.entry, "next"}})
	labels := make([]string, 0, len(b.gotos))
	for l := range b.gotos {
		labels = append(labels, l)
	}
	sort.Strings(labels)
	for _, l := range labels {
		if target, ok := b.labelNode[l]; ok {
			b.connect(b.gotos[l], target, "")
		}
	}
	if b.fin == nil {
		b.connect(out, b.exit, "next")
		return
	}
	b.frames = nil
	b.connect(out, b.fin.node, "next")
	finOut := []pending{{b.fin.node, "next"}}
	if defers > 1 || repeated {
		b.edges = append(b.edges, flowEdge{b.fin.node, b.fin.node, "back"})
	}
	if len(out) > 0 {
		b.connect(finOut, b.exit, "next")
	}
	if b.fin.wasRouted("return") || (b.fin.wasRouted("throw") && recovers) {
		b.connect(finOut, b.exit, "return")
	}
	if b.fin.wasRouted("throw") {
		b.connect(finOut, b.throwExit, "throw")
	}
}

// scan finds, in the body but not in nested function literals, the labels a
// goto targets, how many calls are deferred, whether one is deferred in a
// loop, and whether a deferred call may recover a panic.
func (b *fnBuilder) scan(body *ast.BlockStmt) (defers int, repeated, recovers bool) {
	ast.Inspect(body, func(n ast.Node) bool {
		switch s := n.(type) {
		case *ast.FuncLit:
			return false
		case *ast.BranchStmt:
			if s.Tok == token.GOTO && s.Label != nil {
				b.gotoTargets[s.Label.Name] = true
			}
		case *ast.DeferStmt:
			defers++
			if b.inLoop(s) {
				repeated = true
			}
			if b.mayRecover(s.Call) {
				recovers = true
			}
		}
		return true
	})
	return
}

func (b *fnBuilder) inLoop(n ast.Node) bool {
	for p := b.parent[n]; p != nil; p = b.parent[p] {
		switch p.(type) {
		case *ast.ForStmt, *ast.RangeStmt:
			return true
		case *ast.FuncLit, *ast.FuncDecl:
			return false
		}
	}
	return false
}

// mayRecover: whether a deferred call can stop a panic. `recover` works only
// when the deferred function itself calls it, so a literal or a project
// function is read; the standard library and builtins do not; anything else
// might.
func (b *fnBuilder) mayRecover(call *ast.CallExpr) bool {
	if lit, ok := ast.Unparen(call.Fun).(*ast.FuncLit); ok {
		return callsRecover(b.info, lit.Body)
	}
	ident := calleeIdent(call.Fun)
	if ident == nil {
		return true
	}
	switch o := b.info.Uses[ident].(type) {
	case *types.Builtin:
		return false
	case *types.Func:
		if o.Pkg() != nil && isStandard(o.Pkg().Path()) && !b.x.isProjectPackage(o.Pkg().Path()) {
			return false
		}
		if decl, s := b.x.funcDecl(o); decl != nil {
			return decl.Body != nil && callsRecover(s.pkg.TypesInfo, decl.Body)
		}
	}
	return true
}

func callsRecover(info *types.Info, body *ast.BlockStmt) bool {
	found := false
	ast.Inspect(body, func(n ast.Node) bool {
		switch c := n.(type) {
		case *ast.FuncLit:
			return false
		case *ast.CallExpr:
			if id, ok := ast.Unparen(c.Fun).(*ast.Ident); ok {
				if bi, ok := info.Uses[id].(*types.Builtin); ok && bi.Name() == "recover" {
					found = true
				}
			}
		}
		return !found
	})
	return found
}

// funcDecl is the declaration of a project function, if it has one.
func (x *extractor) funcDecl(f *types.Func) (*ast.FuncDecl, *source) {
	s, ok := x.byAbs[x.fileOf(f.Pos())]
	if !ok {
		return nil, nil
	}
	key := x.posKey(origin(f).Pos())
	for _, d := range s.file.Decls {
		if fd, ok := d.(*ast.FuncDecl); ok && x.posKey(fd.Name.Pos()) == key {
			return fd, s
		}
	}
	return nil, nil
}

// ── statements ──────────────────────────────────────────────────────────────

func (b *fnBuilder) statements(list []ast.Stmt, preds []pending) []pending {
	for _, s := range list {
		preds = b.statement(s, preds, nil)
	}
	return preds
}

func (b *fnBuilder) straight(s ast.Node, preds []pending, exprs ...ast.Node) (int, []pending) {
	n := b.node("stmt", s.Pos(), true)
	b.evaluates(n, exprs...)
	b.connect(preds, n, "")
	return n, []pending{{n, "next"}}
}

func (b *fnBuilder) statement(st ast.Stmt, preds []pending, labels []string) []pending {
	switch s := st.(type) {
	case nil:
		return preds
	case *ast.BlockStmt:
		return b.statements(s.List, preds)
	case *ast.LabeledStmt:
		if b.gotoTargets[s.Label.Name] {
			n := b.node("stmt", s.Pos(), false)
			b.connect(preds, n, "")
			b.labelNode[s.Label.Name] = n
			preds = []pending{{n, "next"}}
		}
		return b.statement(s.Stmt, preds, append(append([]string{}, labels...), s.Label.Name))
	case *ast.EmptyStmt:
		return preds
	case *ast.DeclStmt:
		if gd, ok := s.Decl.(*ast.GenDecl); ok && gd.Tok == token.TYPE {
			return preds
		}
		_, out := b.straight(s, preds, s)
		return out
	case *ast.ExprStmt:
		if isPanic(b.info, s.X) {
			n := b.node("throw", s.Pos(), true)
			b.evaluates(n, s.X)
			b.connect(preds, n, "")
			b.jump([]pending{{n, "throw"}}, "throw", "", len(b.frames))
			return nil
		}
		_, out := b.straight(s, preds, s.X)
		return out
	case *ast.SendStmt:
		n, out := b.straight(s, preds, s.Chan, s.Value)
		b.concurrent(n, "chan_send", s.Pos())
		return out
	case *ast.GoStmt:
		n, out := b.straight(s, preds, s.Call)
		b.concurrent(n, "go", s.Pos())
		return out
	case *ast.DeferStmt:
		// The function value and arguments are evaluated here; the call runs at
		// the finally node.
		exprs := []ast.Node{s.Call.Fun}
		for _, a := range s.Call.Args {
			exprs = append(exprs, a)
		}
		n, out := b.straight(s, preds, exprs...)
		b.concurrent(n, "defer", s.Pos())
		b.deferred = append(b.deferred, s.Call)
		return out
	case *ast.ReturnStmt:
		n := b.node("return", s.Pos(), true)
		for _, r := range s.Results {
			b.evaluates(n, r)
		}
		if len(s.Results) == 0 {
			for _, r := range b.results {
				b.useDeclared(n, r)
			}
		}
		b.connect(preds, n, "")
		b.jump([]pending{{n, "return"}}, "return", "", len(b.frames))
		return nil
	case *ast.BranchStmt:
		label := ""
		if s.Label != nil {
			label = s.Label.Name
		}
		switch s.Tok {
		case token.BREAK, token.CONTINUE:
			kind := s.Tok.String()
			n := b.node(kind, s.Pos(), true)
			b.connect(preds, n, "")
			b.jump([]pending{{n, kind}}, kind, label, len(b.frames))
			return nil
		case token.GOTO:
			n := b.node("goto", s.Pos(), true)
			b.connect(preds, n, "")
			b.gotos[label] = append(b.gotos[label], pending{n, "goto"})
			return nil
		case token.FALLTHROUGH:
			n := b.node("fallthrough", s.Pos(), true)
			b.connect(preds, n, "")
			return []pending{{n, "fallthrough"}}
		}
		return preds
	case *ast.IfStmt:
		if s.Init != nil {
			preds = b.statement(s.Init, preds, nil)
		}
		cond := b.node("cond", s.Cond.Pos(), true)
		b.evaluates(cond, s.Cond)
		b.decision("if", cond, s.Pos(), b.nesting)
		b.connect(preds, cond, "")
		var thenOut, elseOut []pending
		b.nested(func() { thenOut = b.statement(s.Body, []pending{{cond, "on_true"}}, nil) })
		if s.Else != nil {
			b.nested(func() { elseOut = b.statement(s.Else, []pending{{cond, "on_false"}}, nil) })
		} else {
			elseOut = []pending{{cond, "on_false"}}
		}
		return append(thenOut, elseOut...)
	case *ast.ForStmt:
		if s.Init != nil {
			preds = b.statement(s.Init, preds, nil)
		}
		headPos := s.Pos()
		if s.Cond != nil {
			headPos = s.Cond.Pos()
		}
		head := b.node("loop_head", headPos, true)
		if s.Cond != nil {
			b.evaluates(head, s.Cond)
			b.decision("for", head, s.Pos(), b.nesting)
		}
		b.connect(preds, head, "")
		cont, incr := head, -1
		if s.Post != nil {
			incr = b.node("stmt", s.Post.Pos(), true)
			b.evaluates(incr, s.Post)
			cont = incr
		}
		var exits []pending
		if s.Cond != nil {
			exits = []pending{{head, "on_false"}}
		}
		return b.loopBody(s.Body, head, cont, labels, exits, incr)
	case *ast.RangeStmt:
		// The range expression is evaluated once; the head fetches each element and binds it.
		it := b.node("stmt", s.X.Pos(), true)
		b.evaluates(it, s.X)
		b.connect(preds, it, "")
		headPos := s.Pos()
		if s.Key != nil {
			headPos = s.Key.Pos()
		}
		head := b.node("loop_head", headPos, true)
		b.evaluates(head, s.Key, s.Value)
		b.decision("for_of", head, s.Pos(), b.nesting)
		if tv, ok := b.info.Types[s.X]; ok && tv.Type != nil {
			if _, isChan := tv.Type.Underlying().(*types.Chan); isChan {
				b.concurrent(head, "chan_recv", s.Pos())
			}
		}
		b.connect([]pending{{it, "next"}}, head, "")
		return b.loopBody(s.Body, head, head, labels, []pending{{head, "on_false"}}, -1)
	case *ast.SwitchStmt:
		if s.Init != nil {
			preds = b.statement(s.Init, preds, nil)
		}
		pos := s.Pos()
		if s.Tag != nil {
			pos = s.Tag.Pos()
		}
		sw := b.node("switch", pos, true)
		b.evaluates(sw, s.Tag)
		b.connect(preds, sw, "")
		return b.clauses(s.Body, sw, labels, false)
	case *ast.TypeSwitchStmt:
		if s.Init != nil {
			preds = b.statement(s.Init, preds, nil)
		}
		sw := b.node("switch", s.Assign.Pos(), true)
		b.evaluates(sw, s.Assign)
		b.connect(preds, sw, "")
		return b.clauses(s.Body, sw, labels, false)
	case *ast.SelectStmt:
		sel := b.node("select", s.Pos(), true)
		b.concurrent(sel, "select", s.Pos())
		for _, c := range s.Body.List {
			if cc, ok := c.(*ast.CommClause); ok {
				b.evaluates(sel, commOperands(cc.Comm)...)
			}
		}
		b.connect(preds, sel, "")
		if len(s.Body.List) == 0 {
			return nil // `select {}` blocks forever
		}
		return b.clauses(s.Body, sel, labels, true)
	}
	_, out := b.straight(st, preds, st)
	return out
}

func (b *fnBuilder) loopBody(body *ast.BlockStmt, head, cont int, labels []string, exits []pending, incr int) []pending {
	f := &flowFrame{kind: "loop", labels: labels, continueTarget: cont}
	b.frames = append(b.frames, f)
	var out []pending
	b.nested(func() { out = b.statement(body, []pending{{head, "on_true"}}, nil) })
	b.frames = b.frames[:len(b.frames)-1]
	if incr >= 0 {
		b.connect(out, incr, "next")
		b.connect([]pending{{incr, "back"}}, head, "")
	} else {
		b.connect(out, head, "back")
	}
	return append(exits, f.breaks...)
}

// clauses builds a switch, type switch or select body: the cases tested in
// source order, `default` wherever it is written taken when all fail, and
// `fallthrough` carried into the next clause.
func (b *fnBuilder) clauses(body *ast.BlockStmt, sw int, labels []string, isSelect bool) []pending {
	var cases []ast.Stmt
	var deflt ast.Stmt
	for _, c := range body.List {
		switch cc := c.(type) {
		case *ast.CaseClause:
			if cc.List == nil {
				deflt = c
				continue
			}
		case *ast.CommClause:
			if cc.Comm == nil {
				deflt = c
				continue
			}
		}
		cases = append(cases, c)
	}
	entries := map[ast.Stmt][]pending{}
	tests := []pending{{sw, "next"}}
	for i, c := range cases {
		t := b.node("case_test", c.Pos(), true)
		b.connect(tests, t, "")
		switch cc := c.(type) {
		case *ast.CaseClause:
			for _, e := range cc.List {
				b.evaluates(t, e)
			}
		case *ast.CommClause:
			b.comm(t, cc.Comm)
		}
		entries[c] = []pending{{t, "case"}}
		if isSelect && deflt == nil && i == len(cases)-1 {
			tests = nil // nothing else was ready: this one is taken
			continue
		}
		b.decision("case", t, c.Pos(), b.nesting)
		tests = []pending{{t, "on_false"}}
	}
	var out []pending
	if deflt != nil {
		for _, p := range tests {
			entries[deflt] = append(entries[deflt], pending{p.from, "default"})
		}
	} else {
		out = append(out, tests...)
	}
	f := &flowFrame{kind: "switch", labels: labels}
	b.frames = append(b.frames, f)
	var fall []pending
	b.nested(func() {
		for _, c := range body.List {
			into := append(append([]pending{}, entries[c]...), fall...)
			var stmts []ast.Stmt
			switch cc := c.(type) {
			case *ast.CaseClause:
				stmts = cc.Body
			case *ast.CommClause:
				stmts = cc.Body
			}
			// Only a clause ending in `fallthrough` (the one place Go allows it)
			// continues into the next; an empty clause a fallthrough entered is
			// left like any other.
			res := b.statements(stmts, into)
			fall = nil
			if endsInFallthrough(stmts) {
				fall = res
			} else {
				out = append(out, res...)
			}
		}
	})
	b.frames = b.frames[:len(b.frames)-1]
	return append(append(out, fall...), f.breaks...)
}

func endsInFallthrough(stmts []ast.Stmt) bool {
	if len(stmts) == 0 {
		return false
	}
	br, ok := stmts[len(stmts)-1].(*ast.BranchStmt)
	return ok && br.Tok == token.FALLTHROUGH
}

// commOperands: what a select case evaluates on entering the select — its
// channel, and a send's value.
func commOperands(comm ast.Stmt) []ast.Node {
	switch c := comm.(type) {
	case *ast.SendStmt:
		return []ast.Node{c.Chan, c.Value}
	case *ast.ExprStmt:
		if u, ok := ast.Unparen(c.X).(*ast.UnaryExpr); ok && u.Op == token.ARROW {
			return []ast.Node{u.X}
		}
	case *ast.AssignStmt:
		if len(c.Rhs) == 1 {
			if u, ok := ast.Unparen(c.Rhs[0]).(*ast.UnaryExpr); ok && u.Op == token.ARROW {
				return []ast.Node{u.X}
			}
		}
	}
	return nil
}

// comm: what a select case does once taken — the send or receive, and the
// variables a receive assigns.
func (b *fnBuilder) comm(t int, comm ast.Stmt) {
	switch c := comm.(type) {
	case *ast.SendStmt:
		b.concurrent(t, "chan_send", c.Pos())
	case *ast.ExprStmt:
		b.concurrent(t, "chan_recv", c.Pos())
	case *ast.AssignStmt:
		for _, l := range c.Lhs {
			b.evaluates(t, l)
		}
		b.concurrent(t, "chan_recv", c.Pos())
	}
}

func isPanic(info *types.Info, e ast.Expr) bool {
	call, ok := ast.Unparen(e).(*ast.CallExpr)
	if !ok {
		return false
	}
	id, ok := ast.Unparen(call.Fun).(*ast.Ident)
	if !ok {
		return false
	}
	bi, ok := info.Uses[id].(*types.Builtin)
	return ok && bi.Name() == "panic"
}

// ── what each node's expressions do ─────────────────────────────────────────

func (b *fnBuilder) flush() {
	x := b.x
	for _, n := range b.nodes {
		line, col := n.line, n.col
		if line == 0 {
			line, col = x.line(n.pos), x.col(n.pos)
		}
		x.em.emit("flow_node", row{"id": n.id, "fn": b.fnID, "kind": n.kind, "file": b.s.path, "line": line, "col": col})
	}
	for _, e := range b.edges {
		x.em.emit("flow_edge", row{"from": e.from, "to": e.to, "kind": e.kind})
	}
	for _, r := range b.roots {
		b.walk(r.node, r.expr)
	}
	if b.fin != nil {
		for _, c := range b.deferred {
			x.em.emit("call_at", row{"node": b.fin.node, "call_site": x.callSiteID(c)})
		}
	}
	for _, c := range b.concurrency {
		x.em.emit("concurrency_site", c)
	}
}

func (b *fnBuilder) walk(node int, root ast.Node) {
	x, info := b.x, b.info
	nesting := b.nestingOf[node]
	ast.Inspect(root, func(n ast.Node) bool {
		switch e := n.(type) {
		case *ast.FuncLit:
			if id, ok := x.idByKey[x.posKey(e.Pos())+":lit"]; ok {
				x.em.emit("closure", row{"node": node, "fn": id})
			}
			return false
		case *ast.CallExpr:
			if tv, ok := info.Types[e.Fun]; !ok || !tv.IsType() {
				x.em.emit("call_at", row{"node": node, "call_site": x.callSiteID(e)})
			}
		case *ast.BinaryExpr:
			switch e.Op {
			case token.LAND:
				b.decision("and", node, e.Pos(), nesting)
			case token.LOR:
				b.decision("or", node, e.Pos(), nesting)
			}
		case *ast.UnaryExpr:
			if e.Op == token.ARROW {
				b.concurrent(node, "chan_recv", e.Pos())
			}
		case *ast.Ident:
			b.access(node, e)
		}
		return true
	})
}

func (b *fnBuilder) record(rel string, node int, v string) {
	key := rel + "\x00" + strconv.Itoa(node) + "\x00" + v
	if b.recorded[key] {
		return
	}
	b.recorded[key] = true
	b.x.em.emit(rel, row{"node": node, "var": v})
}

// access records a variable's definition or use at `node`, and a capture when
// the variable belongs to an enclosing function.
func (b *fnBuilder) access(node int, id *ast.Ident) {
	if id.Name == "_" {
		return
	}
	info := b.info
	obj, declared := info.Defs[id]
	isDef := declared
	if declared && obj == nil {
		obj = b.symbolicVar(id)
	}
	if !declared {
		obj = info.Uses[id]
	}
	if obj == nil {
		return
	}
	switch obj.(type) {
	case *types.Var, *types.Const:
	default:
		return
	}
	vid, ok := b.x.idOf(obj)
	if !ok || !flowVarKinds[b.x.symbols[vid]["kind"]] {
		return
	}
	def, use := isDef, !isDef
	if !isDef {
		switch b.assignRole(id) {
		case "write":
			def, use = true, false
		case "readwrite":
			def, use = true, true
		}
	}
	if def {
		b.record("def", node, vid)
	}
	if use {
		b.record("use", node, vid)
	}
	b.capture(vid)
}

func (b *fnBuilder) useDeclared(node int, id *ast.Ident) {
	if id.Name == "_" {
		return
	}
	if vid, ok := b.x.idOf(b.info.Defs[id]); ok {
		b.record("use", node, vid)
		b.capture(vid)
	}
}

func (b *fnBuilder) capture(vid string) {
	owner, _ := b.x.symbols[vid]["parent"].(string)
	if owner == "" || owner == b.fnID {
		return
	}
	switch b.x.symbols[owner]["kind"] {
	case "function", "method":
		key := "captures\x00" + vid
		if !b.recorded[key] {
			b.recorded[key] = true
			b.x.em.emit("captures", row{"fn": b.fnID, "var": vid})
		}
	}
}

func (b *fnBuilder) assignRole(id *ast.Ident) string {
	var r ast.Node = id
	for {
		p, ok := b.parent[r].(*ast.ParenExpr)
		if !ok {
			break
		}
		r = p
	}
	switch p := b.parent[r].(type) {
	case *ast.AssignStmt:
		for _, l := range p.Lhs {
			if l == r {
				if p.Tok == token.ASSIGN || p.Tok == token.DEFINE {
					return "write"
				}
				return "readwrite"
			}
		}
	case *ast.IncDecStmt:
		return "readwrite"
	case *ast.RangeStmt:
		if p.Key == r || p.Value == r {
			return "write"
		}
	}
	return "read"
}

// symbolicVar is the object of a type switch's `t` in `switch t := x.(type)`,
// which go/types declares once per clause, all at the identifier.
func (b *fnBuilder) symbolicVar(id *ast.Ident) types.Object {
	as, ok := b.parent[id].(*ast.AssignStmt)
	if !ok {
		return nil
	}
	ts, ok := b.parent[as].(*ast.TypeSwitchStmt)
	if !ok {
		return nil
	}
	for _, c := range ts.Body.List {
		if obj := b.info.Implicits[c]; obj != nil {
			return obj
		}
	}
	return nil
}

// ── per-function metrics ────────────────────────────────────────────────────

type fnMetrics struct {
	statements, maxNesting, cognitive, returns, throws int
}

func (m *fnMetrics) add(o fnMetrics) {
	m.statements += o.statements
	m.maxNesting = max(m.maxNesting, o.maxNesting)
	m.cognitive += o.cognitive
	m.returns += o.returns
	m.throws += o.throws
}

func children(n ast.Node) []ast.Node {
	var out []ast.Node
	ast.Inspect(n, func(c ast.Node) bool {
		if c == n {
			return true
		}
		if c != nil {
			out = append(out, c)
		}
		return false
	})
	return out
}

// measure counts statements, returns and panics, the deepest nesting of
// control structures, and SonarSource cognitive complexity, over a body with
// nested function literals excluded.
func (b *fnBuilder) measure(body ast.Node) fnMetrics {
	m := fnMetrics{}
	var walk func(n ast.Node, depth, cog int)
	walk = func(n ast.Node, depth, cog int) {
		if n == nil {
			return
		}
		if _, lit := n.(*ast.FuncLit); lit {
			return
		}
		if _, ok := n.(ast.Stmt); ok {
			switch n.(type) {
			case *ast.BlockStmt, *ast.CaseClause, *ast.CommClause:
			default:
				m.statements++
			}
		}
		switch e := n.(type) {
		case *ast.ReturnStmt:
			m.returns++
		case *ast.CallExpr:
			if isPanic(b.info, e) {
				m.throws++
			}
		case *ast.BranchStmt:
			if e.Tok == token.GOTO || (e.Label != nil && (e.Tok == token.BREAK || e.Tok == token.CONTINUE)) {
				m.cognitive++
			}
		case *ast.BinaryExpr:
			if e.Op == token.LAND || e.Op == token.LOR {
				p := b.parent[n]
				for {
					pp, ok := p.(*ast.ParenExpr)
					if !ok {
						break
					}
					p = b.parent[pp]
				}
				if pb, ok := p.(*ast.BinaryExpr); !ok || pb.Op != e.Op {
					m.cognitive++
				}
			}
		case *ast.IfStmt:
			elseIf := false
			if p, ok := b.parent[n].(*ast.IfStmt); ok && p.Else == n {
				elseIf = true
			}
			d := depth
			if !elseIf {
				d = depth + 1
				m.maxNesting = max(m.maxNesting, d)
				m.cognitive += 1 + cog
			} else {
				m.cognitive++
			}
			_, chained := e.Else.(*ast.IfStmt)
			if e.Else != nil && !chained {
				m.cognitive++
			}
			walk(e.Init, d, cog)
			walk(e.Cond, d, cog)
			walk(e.Body, d, cog+1)
			if chained {
				walk(e.Else, d, cog)
			} else if e.Else != nil {
				walk(e.Else, d, cog+1)
			}
			return
		case *ast.ForStmt, *ast.RangeStmt, *ast.SwitchStmt, *ast.TypeSwitchStmt, *ast.SelectStmt:
			d := depth + 1
			m.maxNesting = max(m.maxNesting, d)
			m.cognitive += 1 + cog
			for _, c := range children(n) {
				walk(c, d, cog+1)
			}
			return
		}
		for _, c := range children(n) {
			walk(c, depth, cog)
		}
	}
	for _, c := range children(body) {
		walk(c, 0, 0)
	}
	return m
}

// ── Halstead ────────────────────────────────────────────────────────────────

type srcToken struct {
	line, col int
	tok       token.Token
	lit       string
}

func fileTokens(text []byte) []srcToken {
	fset := token.NewFileSet()
	f := fset.AddFile("", -1, len(text))
	var sc scanner.Scanner
	sc.Init(f, text, func(token.Position, string) {}, 0)
	var out []srcToken
	for {
		pos, tok, lit := sc.Scan()
		if tok == token.EOF {
			break
		}
		if tok == token.SEMICOLON && lit == "\n" {
			continue
		}
		p := f.Position(pos)
		out = append(out, srcToken{p.Line, p.Column, tok, lit})
	}
	return out
}

type halsteadCounts struct {
	operators, operands, distinctOperators, distinctOperands int
}

type span struct{ from, to [2]int }

func before(a, b [2]int) bool { return a[0] < b[0] || (a[0] == b[0] && a[1] < b[1]) }

// halstead counts the token leaves of a body — or, for a file's <module>, of
// everything outside its functions — nested function literals excluded.
func halstead(x *extractor, s *source, toks []srcToken, from, to token.Pos, body ast.Node) halsteadCounts {
	at := func(p token.Pos) [2]int { return [2]int{x.line(p), x.col(p)} }
	var lo, hi [2]int
	var holes []span
	if body == nil {
		lo, hi = [2]int{0, 0}, [2]int{1 << 30, 0}
		for _, d := range s.file.Decls {
			if fd, ok := d.(*ast.FuncDecl); ok {
				holes = append(holes, span{at(fd.Pos()), at(fd.End())})
			}
		}
		body = s.file
	} else {
		lo, hi = at(from), at(to)
	}
	ast.Inspect(body, func(n ast.Node) bool {
		if lit, ok := n.(*ast.FuncLit); ok {
			holes = append(holes, span{at(lit.Pos()), at(lit.End())})
			return false
		}
		return true
	})
	h := halsteadCounts{}
	ops, opnds := map[string]bool{}, map[string]bool{}
	for _, t := range toks {
		p := [2]int{t.line, t.col}
		if before(p, lo) || !before(p, hi) {
			continue
		}
		inHole := false
		for _, sp := range holes {
			if !before(p, sp.from) && before(p, sp.to) {
				inHole = true
				break
			}
		}
		if inHole {
			continue
		}
		switch t.tok {
		case token.IDENT, token.INT, token.FLOAT, token.IMAG, token.CHAR, token.STRING:
			h.operands++
			opnds[t.lit] = true
		default:
			h.operators++
			ops[t.tok.String()] = true
		}
	}
	h.distinctOperators, h.distinctOperands = len(ops), len(opnds)
	return h
}

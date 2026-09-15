package main

import (
	"go/ast"
	"go/parser"
	"go/token"
	"go/types"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"

	"golang.org/x/tools/go/packages"
)

// The quality layer: what loading and type-checking said about the code, and
// the places its authors suppressed, deferred or overrode something —
// diagnostics, lint suppressions, compiler directives, TODO markers, where
// `any` enters, type assertions, literals, panic and recover sites, and error
// results nothing reads.

var (
	commentMarker = regexp.MustCompile(`\b(TODO|FIXME|HACK|XXX)\b[:\s-]*(.*)`)
	nolint        = regexp.MustCompile(`^//nolint(?::([\w,.\-]+))?`)
	lintIgnore    = regexp.MustCompile(`^//lint:(ignore|file-ignore)\s+(\S+)`)
	nosec         = regexp.MustCompile(`#nosec\b\s*([A-Z0-9][A-Z0-9, ]*)?`)
	goDirective   = regexp.MustCompile(`^//(go:[a-z_]+|export|line)(?:\s+(.*))?$`)
)

func (x *extractor) emitQuality() {
	x.emitDiagnostics()
	errorIface := types.Universe.Lookup("error").Type().Underlying().(*types.Interface)
	for _, s := range x.sources {
		x.emitComments(s)
		w := &refWalker{x: x, s: s, info: s.pkg.TypesInfo, parent: parents(s.file)}
		w.quality(errorIface)
	}
}

// emitDiagnostics writes what go/packages reported: the go command's listing
// errors, parse errors and type errors, once each though a test variant
// type-checks its package again.
func (x *extractor) emitDiagnostics() {
	seen := map[string]bool{}
	for _, p := range x.roots {
		typeErrors := false
		for _, e := range p.Errors {
			if e.Kind == packages.TypeError {
				typeErrors = true
			}
		}
		for _, e := range p.Errors {
			message := e.Msg
			// The go command echoes a failed compile as `# <package>` and the
			// compiler's lines: the type checker has said each already, or the
			// first of those lines is the message.
			if header, rest, ok := strings.Cut(message, "\n"); ok && strings.HasPrefix(header, "# ") {
				if typeErrors {
					continue
				}
				message = rest
			}
			key := e.Pos + "\x00" + message
			if seen[key] {
				continue
			}
			seen[key] = true
			file, line := x.errorPosition(e.Pos)
			message, _, _ = strings.Cut(message, "\n")
			r := row{"file": nullable(file), "line": nil, "code": 0, "category": "error", "message": truncate(message, 300)}
			if file != "" && line > 0 {
				r["line"] = line
			}
			x.em.emit("diagnostic", r)
		}
	}
}

// errorPosition reads go/packages' `file:line:col`; a position outside the
// files extracted is no position.
func (x *extractor) errorPosition(pos string) (string, int) {
	if pos == "" || pos == "-" {
		return "", 0
	}
	parts := strings.Split(pos, ":")
	n := len(parts)
	for n > 1 {
		if _, err := strconv.Atoi(parts[n-1]); err != nil {
			break
		}
		n--
	}
	s, ok := x.byAbs[filepath.Clean(strings.Join(parts[:n], ":"))]
	if !ok {
		return "", 0
	}
	line := 0
	if n < len(parts) {
		line, _ = strconv.Atoi(parts[n])
	}
	return s.path, line
}

// emitComments reads the comments of the file as written: lint suppressions,
// directives (a `//` comment with no space before the name), and markers.
func (x *extractor) emitComments(s *source) {
	fset := token.NewFileSet()
	f, _ := parser.ParseFile(fset, s.abs, s.text, parser.ParseComments|parser.SkipObjectResolution)
	if f == nil {
		return
	}
	for _, g := range f.Comments {
		for _, c := range g.List {
			start := fset.Position(c.Pos()).Line
			if m := goDirective.FindStringSubmatch(c.Text); m != nil {
				x.em.emit("compiler_directive", row{"file": s.path, "line": start, "name": strings.TrimPrefix(m[1], "go:"), "text": nullable(truncate(m[2], 200))})
			}
			if m := nolint.FindStringSubmatch(c.Text); m != nil {
				x.em.emit("lint_directive", row{"file": s.path, "line": start, "tool": "golangci", "directive": "nolint", "rules": nullable(m[1])})
			}
			if m := lintIgnore.FindStringSubmatch(c.Text); m != nil {
				x.em.emit("lint_directive", row{"file": s.path, "line": start, "tool": "staticcheck", "directive": m[1], "rules": nullable(m[2])})
			}
			for i, text := range strings.Split(c.Text, "\n") {
				if m := nosec.FindStringSubmatch(text); m != nil {
					x.em.emit("lint_directive", row{"file": s.path, "line": start + i, "tool": "gosec", "directive": "nosec", "rules": nullable(strings.TrimSpace(m[1]))})
				}
				if m := commentMarker.FindStringSubmatch(text); m != nil {
					x.em.emit("comment_marker", row{"file": s.path, "line": start + i, "kind": strings.ToLower(m[1]), "text": truncate(strings.TrimSuffix(m[2], "*/"), 200)})
				}
			}
		}
	}
}

func isEmptyInterface(t types.Type) bool {
	if t == nil {
		return false
	}
	if _, isParam := types.Unalias(t).(*types.TypeParam); isParam {
		return false
	}
	it, ok := t.Underlying().(*types.Interface)
	return ok && it.Empty()
}

func (w *refWalker) quality(errorIface *types.Interface) {
	x, info, s := w.x, w.info, w.s
	typeOf := func(e ast.Expr) types.Type {
		if tv, ok := info.Types[e]; ok {
			return tv.Type
		}
		return nil
	}
	isError := func(t types.Type) bool { return t != nil && types.Implements(t, errorIface) }
	isCall := func(c *ast.CallExpr) bool {
		tv, ok := info.Types[c.Fun]
		return !ok || !tv.IsType()
	}
	// results is a call's result types, one per value.
	results := func(c *ast.CallExpr) []types.Type {
		t := typeOf(c)
		if tuple, ok := t.(*types.Tuple); ok {
			out := make([]types.Type, tuple.Len())
			for i := range out {
				out[i] = tuple.At(i).Type()
			}
			return out
		}
		if t == nil {
			return nil
		}
		return []types.Type{t}
	}
	dropped := func(c *ast.CallExpr, how string) {
		for _, t := range results(c) {
			if isError(t) {
				x.em.emit("ignored_error", row{"call_site": x.callSiteID(c), "fn": w.ownerOf(c), "file": s.path, "line": x.line(c.Pos()), "how": how})
				return
			}
		}
	}

	ast.Inspect(s.file, func(n ast.Node) bool {
		if n == nil {
			return false
		}
		if x.fileOf(n.Pos()) != s.abs {
			return true // declarations cgo generated
		}
		switch e := n.(type) {
		case *ast.ImportSpec:
			return false
		case *ast.Field:
			if e.Tag != nil {
				ast.Inspect(e.Type, func(m ast.Node) bool { w.qualityNode(m, typeOf, isCall); return true })
				return false
			}
		case *ast.ArrayType:
			// `[4]int`: a length in a type is no value anyone reads.
			ast.Inspect(e.Elt, func(m ast.Node) bool { w.qualityNode(m, typeOf, isCall); return true })
			return false
		case *ast.ExprStmt:
			if c, ok := ast.Unparen(e.X).(*ast.CallExpr); ok && isCall(c) {
				dropped(c, "discarded")
			}
		case *ast.DeferStmt:
			dropped(e.Call, "deferred")
		case *ast.GoStmt:
			dropped(e.Call, "go")
		case *ast.AssignStmt:
			if len(e.Rhs) == 1 {
				if c, ok := ast.Unparen(e.Rhs[0]).(*ast.CallExpr); ok && isCall(c) {
					rs := results(c)
					for i, l := range e.Lhs {
						if id, ok := l.(*ast.Ident); ok && id.Name == "_" && i < len(rs) && isError(rs[i]) {
							x.em.emit("ignored_error", row{"call_site": x.callSiteID(c), "fn": w.ownerOf(c), "file": s.path, "line": x.line(c.Pos()), "how": "blank"})
							break
						}
					}
				}
			}
		}
		w.qualityNode(n, typeOf, isCall)
		return true
	})
}

// qualityNode records what one node is: a literal, `any`, an assertion, a
// panic or a recover.
func (w *refWalker) qualityNode(n ast.Node, typeOf func(ast.Expr) types.Type, isCall func(*ast.CallExpr) bool) {
	x, info, s := w.x, w.info, w.s
	switch e := n.(type) {
	case *ast.BasicLit:
		kind, value := "number", e.Value
		switch e.Kind {
		case token.STRING, token.CHAR:
			kind = "string"
			if v, err := strconv.Unquote(e.Value); err == nil {
				value = v
			}
		}
		x.em.emit("literal", row{"fn": w.ownerOf(e), "file": s.path, "line": x.line(e.Pos()), "kind": kind, "value": truncate(value, 100)})
	case *ast.Ident:
		if tn, ok := info.Uses[e].(*types.TypeName); ok && tn.Pkg() == nil && tn.Name() == "any" && !w.inTypeParams(e) {
			x.em.emit("any_site", row{"fn": w.ownerOf(e), "file": s.path, "line": x.line(e.Pos()), "kind": "explicit"})
		}
	case *ast.InterfaceType:
		if len(e.Methods.List) == 0 && !w.inTypeParams(e) {
			x.em.emit("any_site", row{"fn": w.ownerOf(e), "file": s.path, "line": x.line(e.Pos()), "kind": "explicit"})
		}
	case *ast.CallExpr:
		if !isCall(e) {
			return
		}
		if isEmptyInterface(typeOf(e)) {
			x.em.emit("any_site", row{"fn": w.ownerOf(e), "file": s.path, "line": x.line(e.Pos()), "kind": "call_result"})
		}
		id, ok := ast.Unparen(e.Fun).(*ast.Ident)
		if !ok {
			return
		}
		bi, ok := info.Uses[id].(*types.Builtin)
		if !ok {
			return
		}
		switch bi.Name() {
		case "panic":
			var thrown any
			if len(e.Args) == 1 {
				if named := namedOf(typeOf(e.Args[0])); named != nil {
					if tid, ok := x.targetID(named.Obj()); ok {
						thrown = tid
					}
				}
			}
			x.em.emit("throw_site", row{"fn": w.ownerOf(e), "file": s.path, "line": x.line(e.Pos()), "type": thrown})
		case "recover":
			_, discarded := w.parent[e].(*ast.ExprStmt)
			body := w.enclosingBody(e)
			empty, rethrows := false, false
			if body != nil {
				// Empty: the function is nothing but `recover()`, swallowing the panic.
				if len(body.List) == 1 {
					if st, ok := body.List[0].(*ast.ExprStmt); ok && ast.Unparen(st.X) == e {
						empty = true
					}
				}
				ast.Inspect(body, func(m ast.Node) bool {
					switch c := m.(type) {
					case *ast.FuncLit:
						return false
					case *ast.CallExpr:
						if isPanic(info, c) {
							rethrows = true
						}
					}
					return !rethrows
				})
			}
			x.em.emit("catch_site", row{"fn": w.ownerOf(e), "file": s.path, "line": x.line(e.Pos()), "binds": !discarded, "empty": empty, "rethrows": rethrows})
		}
	case *ast.TypeAssertExpr:
		if e.Type == nil {
			return // a type switch's `x.(type)`: recorded at the switch
		}
		x.em.emit("assertion", row{
			"fn": w.ownerOf(e), "file": s.path, "line": x.line(e.Pos()), "kind": "type_assert",
			"to_type": truncate(types.ExprString(e.Type), 200), "from_any": isEmptyInterface(typeOf(e.X)), "to_any": isEmptyInterface(typeOf(e.Type)),
		})
	case *ast.TypeSwitchStmt:
		var subject ast.Expr
		switch a := e.Assign.(type) {
		case *ast.AssignStmt:
			if len(a.Rhs) == 1 {
				if ta, ok := a.Rhs[0].(*ast.TypeAssertExpr); ok {
					subject = ta.X
				}
			}
		case *ast.ExprStmt:
			if ta, ok := a.X.(*ast.TypeAssertExpr); ok {
				subject = ta.X
			}
		}
		fromAny := subject != nil && isEmptyInterface(typeOf(subject))
		x.em.emit("assertion", row{"fn": w.ownerOf(e), "file": s.path, "line": x.line(e.Pos()), "kind": "type_switch", "to_type": nil, "from_any": fromAny, "to_any": false})
	}
}

// inTypeParams: `any` written as a type parameter's constraint says nothing
// about a value.
func (w *refWalker) inTypeParams(n ast.Node) bool {
	for p := w.parent[n]; p != nil; p = w.parent[p] {
		fl, ok := p.(*ast.FieldList)
		if !ok {
			continue
		}
		switch g := w.parent[fl].(type) {
		case *ast.FuncType:
			return g.TypeParams == fl
		case *ast.TypeSpec:
			return g.TypeParams == fl
		}
		return false
	}
	return false
}

func (w *refWalker) enclosingBody(n ast.Node) *ast.BlockStmt {
	for p := w.parent[n]; p != nil; p = w.parent[p] {
		switch f := p.(type) {
		case *ast.FuncLit:
			return f.Body
		case *ast.FuncDecl:
			return f.Body
		}
	}
	return nil
}

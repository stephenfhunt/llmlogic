package main

import (
	"fmt"
	"go/ast"
	"go/token"
	"go/types"
	"path/filepath"
	"strconv"
	"strings"
)

// Ids are keyed by declaration position, not by types.Object: a file is
// type-checked once in its package and again in the package's test variant, and
// both objects are one declaration. Ids are assigned in path order, so a
// collision suffix (`@line`) lands on the same declaration however the packages
// were listed.

// Positions are read through `//line` directives: the type checker sees cgo's
// rewrite of a file in the build cache, and the directives point back to the
// file on disk.
func (x *extractor) posKey(pos token.Pos) string {
	p := x.fset.Position(pos)
	return filepath.Clean(p.Filename) + ":" + strconv.Itoa(p.Line) + ":" + strconv.Itoa(p.Column)
}

func (x *extractor) fileOf(pos token.Pos) string {
	return filepath.Clean(x.fset.Position(pos).Filename)
}

func (x *extractor) line(pos token.Pos) int {
	return x.fset.Position(pos).Line
}

func (x *extractor) col(pos token.Pos) int {
	return x.fset.Position(pos).Column
}

func (x *extractor) claim(candidate, key string, pos token.Pos) string {
	tries := []string{candidate, fmt.Sprintf("%s@%d", candidate, x.line(pos)), fmt.Sprintf("%s@%d:%d", candidate, x.line(pos), x.col(pos))}
	for _, t := range tries {
		if holder, ok := x.keyByID[t]; !ok || holder == key {
			x.keyByID[t] = key
			return t
		}
	}
	for n := 2; ; n++ {
		t := fmt.Sprintf("%s#%d", tries[2], n)
		if _, ok := x.keyByID[t]; !ok {
			x.keyByID[t] = key
			return t
		}
	}
}

func moduleID(s *source) string { return s.path + "#<module>" }

// member is a container's id joined to a segment: `path#Name` under a file's
// <module>, `Container.name` under anything else.
func member(container, segment string) string {
	if strings.HasSuffix(container, "#<module>") {
		return strings.TrimSuffix(container, "<module>") + segment
	}
	return container + "." + segment
}

type declared struct {
	kind, form, visibility string
	exported, abstract     bool
	readonly, ambient      bool
}

// describe maps a Go object onto the schema's shared kinds; `form` keeps the
// construct's own name where the kind is only the nearest shared one.
func describe(obj types.Object, s *source) (declared, bool) {
	d := declared{}
	switch o := obj.(type) {
	case *types.TypeName:
		switch {
		case isTypeParam(o):
			d.kind = "type_parameter"
		case o.IsAlias():
			d.kind = "type_alias"
		default:
			switch o.Type().Underlying().(type) {
			case *types.Struct:
				d.kind, d.form = "class", "struct"
			case *types.Interface:
				d.kind = "interface"
			default:
				d.kind, d.form = "class", "defined_type"
			}
		}
	case *types.Func:
		sig, _ := o.Type().(*types.Signature)
		if sig != nil && sig.Recv() != nil {
			d.kind = "method"
			if _, isIface := sig.Recv().Type().Underlying().(*types.Interface); isIface {
				d.abstract = true
			} else if _, ptr := types.Unalias(sig.Recv().Type()).(*types.Pointer); ptr {
				d.form = "pointer_receiver"
			} else {
				d.form = "value_receiver"
			}
			d.visibility = visibilityOf(o.Name())
		} else {
			d.kind = "function"
		}
	case *types.Var:
		switch o.Kind() {
		case types.FieldVar:
			d.kind = "property"
			if o.Embedded() {
				d.form = "embedded"
			}
			d.visibility = visibilityOf(o.Name())
		case types.PackageVar:
			d.kind = "variable"
		case types.RecvVar, types.ParamVar:
			d.kind = "parameter"
		default:
			d.kind = "local"
		}
	case *types.Const:
		d.readonly = true
		if o.Parent() != nil && o.Pkg() != nil && o.Parent() == o.Pkg().Scope() {
			d.kind = "variable"
		} else {
			d.kind = "local"
		}
	default:
		return d, false
	}
	if obj.Pkg() != nil && obj.Parent() == obj.Pkg().Scope() {
		d.exported = obj.Exported()
	}
	return d, true
}

func isTypeParam(o *types.TypeName) bool {
	_, ok := o.Type().(*types.TypeParam)
	return ok
}

func visibilityOf(name string) string {
	if token.IsExported(name) {
		return "public"
	}
	return "package"
}

func (x *extractor) addSymbol(id, name string, d declared, s *source, parent string, line, endLine int) {
	if _, ok := x.symbols[id]; ok {
		return
	}
	r := row{
		"id": id, "name": name, "kind": d.kind, "origin": "project",
		"file": s.path, "line": line, "end_line": endLine, "parent": nullable(parent),
		"package": nullable(modulePath(s)), "exported": d.exported, "visibility": nullable(d.visibility),
		"is_static": false, "is_abstract": d.abstract, "is_async": false, "is_generator": false,
		"is_readonly": d.readonly, "is_optional": false, "is_ambient": d.ambient, "form": nullable(d.form),
	}
	x.symbols[id] = r
	x.symOrder = append(x.symOrder, id)
}

func (x *extractor) addExternal(id, name, kind, origin, pkg, parent string) {
	if _, ok := x.symbols[id]; ok {
		return
	}
	x.symbols[id] = row{
		"id": id, "name": name, "kind": kind, "origin": origin,
		"file": nil, "line": nil, "end_line": nil, "parent": nullable(parent),
		"package": nullable(pkg), "exported": false, "visibility": nil,
		"is_static": false, "is_abstract": false, "is_async": false, "is_generator": false,
		"is_readonly": false, "is_optional": false, "is_ambient": true, "form": nil,
	}
	x.symOrder = append(x.symOrder, id)
}

func (x *extractor) flushSymbols() {
	for _, id := range x.symOrder {
		x.em.emit("symbol", x.symbols[id])
	}
}

func nullable(s string) any {
	if s == "" {
		return nil
	}
	return s
}

func modulePath(s *source) string {
	if s.module == nil {
		return ""
	}
	return s.module.path
}

// idOf is the id of a project object's declaration, if one was assigned.
func (x *extractor) idOf(obj types.Object) (string, bool) {
	if obj == nil {
		return "", false
	}
	id, ok := x.idByKey[x.posKey(obj.Pos())]
	return id, ok
}

// declare registers a project object under `container`.
func (x *extractor) declare(obj types.Object, s *source, container string, node ast.Node) (string, bool) {
	key := x.posKey(obj.Pos())
	if id, ok := x.idByKey[key]; ok {
		return id, true
	}
	// A declaration cgo generated has no place in the file written.
	if obj.Name() == "_" || x.fileOf(obj.Pos()) != s.abs {
		return "", false
	}
	d, ok := describe(obj, s)
	if !ok {
		return "", false
	}
	id := x.claim(member(container, obj.Name()), key, obj.Pos())
	x.idByKey[key] = id
	x.objOf[id] = obj
	x.viewOf[id] = s.pkg.Types
	end := x.line(obj.Pos())
	if node != nil {
		end = x.line(node.End())
	}
	x.addSymbol(id, obj.Name(), d, s, container, x.line(obj.Pos()), end)
	return id, true
}

// assignIDs walks every file in path order: first each package-level
// declaration, so a method in one file can hang off a type declared in a later
// one, then everything inside them.
func (x *extractor) assignIDs() {
	for _, s := range x.sources {
		key := s.path + ":<module>"
		id := x.claim(moduleID(s), key, s.file.Package)
		x.idByKey[key] = id
		x.addSymbol(id, "<module>", declared{kind: "module"}, s, "", 1, lineCount(s.text))
	}
	x.assignPackageSymbols()
	for _, s := range x.sources {
		info := s.pkg.TypesInfo
		for _, decl := range s.file.Decls {
			switch d := decl.(type) {
			case *ast.GenDecl:
				for _, spec := range d.Specs {
					switch sp := spec.(type) {
					case *ast.TypeSpec:
						x.declareIdent(info, sp.Name, s, moduleID(s), sp)
					case *ast.ValueSpec:
						for i, n := range sp.Names {
							x.declareIdent(info, n, s, moduleID(s), valueNode(sp, i))
						}
					}
				}
			case *ast.FuncDecl:
				if d.Recv == nil {
					x.declareIdent(info, d.Name, s, moduleID(s), d)
				}
			}
		}
	}
	for _, s := range x.sources {
		v := &idVisitor{x: x, s: s, container: moduleID(s)}
		ast.Walk(v, s.file)
	}
}

func valueNode(sp *ast.ValueSpec, i int) ast.Node {
	if len(sp.Names) == len(sp.Values) {
		if lit, ok := sp.Values[i].(*ast.FuncLit); ok {
			return lit
		}
	}
	return sp.Names[i]
}

func (x *extractor) declareIdent(info *types.Info, ident *ast.Ident, s *source, container string, node ast.Node) (string, bool) {
	obj := info.Defs[ident]
	if obj == nil {
		return "", false
	}
	id, ok := x.declare(obj, s, container, node)
	if ok {
		if lit, isLit := node.(*ast.FuncLit); isLit {
			x.adopt(id, lit)
		}
	}
	return id, ok
}

// adopt makes a function literal bound by a declaration that declaration:
// `f := func() {…}` is `f`, the name every caller uses.
func (x *extractor) adopt(id string, lit *ast.FuncLit) {
	x.idByKey[x.posKey(lit.Pos())+":lit"] = id
	if r, ok := x.symbols[id]; ok {
		r["kind"] = "function"
	}
}

// assignPackageSymbols gives each project package a symbol, `dir#<package>`, the
// target of an import naming it. An external test package (`package p_test`
// beside `p`) shares the directory and is `dir#<package_test>`.
func (x *extractor) assignPackageSymbols() {
	for _, s := range x.sources {
		path := s.pkg.Types.Path()
		if _, ok := x.pkgSymbol[path]; ok {
			continue
		}
		dir := parentDir(s.path)
		segment := "<package>"
		if strings.HasSuffix(path, "_test") && strings.HasSuffix(s.pkg.Types.Name(), "_test") {
			segment = "<package_test>"
		}
		key := "package:" + path
		id := x.claim(dir+"#"+segment, key, s.file.Package)
		x.idByKey[key] = id
		x.pkgSymbol[path] = id
		d := declared{kind: "namespace"}
		x.addSymbol(id, s.pkg.Types.Name(), d, s, "", x.line(s.file.Package), x.line(s.file.Package))
	}
}

func parentDir(p string) string {
	if i := strings.LastIndex(p, "/"); i >= 0 {
		return p[:i]
	}
	return "."
}

func lineCount(text []byte) int {
	n := strings.Count(string(text), "\n")
	if len(text) > 0 && text[len(text)-1] != '\n' {
		n++
	}
	return n
}

type idVisitor struct {
	x         *extractor
	s         *source
	container string
}

func (v *idVisitor) with(container string) *idVisitor {
	return &idVisitor{x: v.x, s: v.s, container: container}
}

func (v *idVisitor) Visit(n ast.Node) ast.Visitor {
	x, s, info := v.x, v.s, v.s.pkg.TypesInfo
	switch n := n.(type) {
	case nil:
		return nil
	case *ast.FuncDecl:
		container := moduleID(s)
		if n.Recv != nil && len(n.Recv.List) > 0 {
			if base := receiverBase(n.Recv.List[0].Type); base != nil {
				if id, ok := x.idOf(info.Uses[base]); ok {
					container = id
				}
			}
		}
		id, ok := x.declareIdent(info, n.Name, s, container, n)
		if !ok {
			return v
		}
		x.declareUnnamedParams(info.Defs[n.Name], s, id)
		return v.with(id)
	case *ast.FuncLit:
		key := x.posKey(n.Pos()) + ":lit"
		id, ok := x.idByKey[key]
		if !ok {
			segment := fmt.Sprintf("<function@%d:%d>", x.line(n.Pos()), x.col(n.Pos()))
			id = x.claim(member(v.container, segment), key, n.Pos())
			x.idByKey[key] = id
			x.addSymbol(id, "<function>", declared{kind: "function"}, s, v.container, x.line(n.Pos()), x.line(n.End()))
			if tv, ok := info.Types[n]; ok {
				x.litTypes[id] = tv.Type
				x.viewOf[id] = s.pkg.Types
			}
		}
		if tv, ok := info.Types[n]; ok {
			x.declareUnnamedSignature(tv.Type, s, id)
		}
		return v.with(id)
	case *ast.TypeSpec:
		id, ok := x.declareIdent(info, n.Name, s, v.container, n)
		if !ok {
			return v
		}
		return v.with(id)
	case *ast.StructType:
		for _, f := range n.Fields.List {
			inner := v.container
			for i, name := range f.Names {
				if id, ok := x.declareIdent(info, name, s, v.container, f); ok && i == 0 {
					inner = id
				}
			}
			if len(f.Names) == 0 {
				if ident := embeddedIdent(f.Type); ident != nil {
					x.declareIdent(info, ident, s, v.container, f)
				}
			}
			ast.Walk(v.with(inner), f.Type)
		}
		return nil
	case *ast.InterfaceType:
		for _, f := range n.Methods.List {
			for _, name := range f.Names {
				if id, ok := x.declareIdent(info, name, s, v.container, f); ok {
					x.declareUnnamedParams(info.Defs[name], s, id)
					ast.Walk(v.with(id), f.Type)
				}
			}
			if len(f.Names) == 0 {
				ast.Walk(v, f.Type)
			}
		}
		return nil
	case *ast.ValueSpec:
		for i, name := range n.Names {
			x.declareIdent(info, name, s, v.container, valueNode(n, i))
		}
		return v
	case *ast.AssignStmt:
		if n.Tok == token.DEFINE {
			for i, lhs := range n.Lhs {
				ident, ok := lhs.(*ast.Ident)
				if !ok {
					continue
				}
				var node ast.Node = ident
				if len(n.Lhs) == len(n.Rhs) {
					if lit, isLit := n.Rhs[i].(*ast.FuncLit); isLit {
						node = lit
					}
				}
				x.declareIdent(info, ident, s, v.container, node)
			}
		}
		return v
	case *ast.TypeSwitchStmt:
		// `switch t := x.(type)` declares one `t` per clause (info.Implicits),
		// every one at the identifier: one declaration, one id.
		if as, ok := n.Assign.(*ast.AssignStmt); ok && len(as.Lhs) == 1 {
			if ident, ok := as.Lhs[0].(*ast.Ident); ok && ident.Name != "_" {
				for _, stmt := range n.Body.List {
					if obj := info.Implicits[stmt]; obj != nil {
						x.declare(obj, s, v.container, ident)
						break
					}
				}
			}
		}
		return v
	case *ast.Ident:
		if obj := info.Defs[n]; obj != nil {
			x.declare(obj, s, v.container, nil)
		}
		return nil
	}
	return v
}

// declareUnnamedParams gives a parameter with no name (`func(int)`) an id of its
// own, `F.<param@I>`, so every `param` row has a symbol.
func (x *extractor) declareUnnamedParams(obj types.Object, s *source, fnID string) {
	if obj == nil {
		return
	}
	x.declareUnnamedSignature(obj.Type(), s, fnID)
}

func (x *extractor) declareUnnamedSignature(t types.Type, s *source, fnID string) {
	sig, ok := t.(*types.Signature)
	if !ok {
		return
	}
	for i := 0; i < sig.Params().Len(); i++ {
		p := sig.Params().At(i)
		if p.Name() != "" || !p.Pos().IsValid() {
			continue
		}
		key := x.posKey(p.Pos()) + ":param"
		if _, ok := x.idByKey[key]; ok {
			continue
		}
		id := x.claim(member(fnID, fmt.Sprintf("<param@%d>", i)), key, p.Pos())
		x.idByKey[key] = id
		x.objOf[id] = p
		x.viewOf[id] = s.pkg.Types
		x.addSymbol(id, "<unnamed>", declared{kind: "parameter"}, s, fnID, x.line(p.Pos()), x.line(p.Pos()))
	}
}

// paramID is the id of a signature's parameter, named or not.
func (x *extractor) paramID(p *types.Var) (string, bool) {
	if p.Name() == "" {
		id, ok := x.idByKey[x.posKey(p.Pos())+":param"]
		return id, ok
	}
	return x.idOf(p)
}

// receiverBase is the type name a method's receiver names: `T` in `(t *T[K])`.
func receiverBase(e ast.Expr) *ast.Ident {
	for {
		switch t := e.(type) {
		case *ast.StarExpr:
			e = t.X
		case *ast.ParenExpr:
			e = t.X
		case *ast.IndexExpr:
			e = t.X
		case *ast.IndexListExpr:
			e = t.X
		case *ast.Ident:
			return t
		default:
			return nil
		}
	}
}

// embeddedIdent is the identifier an embedded field declares: `T` in `*pkg.T[K]`.
func embeddedIdent(e ast.Expr) *ast.Ident {
	for {
		switch t := e.(type) {
		case *ast.StarExpr:
			e = t.X
		case *ast.IndexExpr:
			e = t.X
		case *ast.IndexListExpr:
			e = t.X
		case *ast.SelectorExpr:
			return t.Sel
		case *ast.Ident:
			return t
		default:
			return nil
		}
	}
}

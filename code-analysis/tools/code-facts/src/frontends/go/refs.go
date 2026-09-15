package main

import (
	"go/ast"
	"go/token"
	"go/types"
	"sort"
	"strings"
)

// The refs layer: every name the type checker resolved, as a `ref` edge from
// its enclosing declaration; every call with its target and how it dispatches;
// struct and interface embedding; the interfaces each type satisfies and the
// methods that satisfy them; member access, type positions and types.

type refWalker struct {
	x      *extractor
	s      *source
	info   *types.Info
	parent map[ast.Node]ast.Node
}

func (x *extractor) emitRefs() {
	for _, s := range x.sources {
		w := &refWalker{x: x, s: s, info: s.pkg.TypesInfo, parent: parents(s.file)}
		ast.Inspect(s.file, func(n ast.Node) bool {
			switch n := n.(type) {
			case *ast.ImportSpec:
				return false
			case *ast.Ident:
				w.ident(n)
			case *ast.CallExpr:
				w.call(n)
			case *ast.StructType:
				w.embedding(n)
			case *ast.InterfaceType:
				w.interfaceEmbedding(n)
			}
			return true
		})
	}
	x.emitImplementations()
	x.emitSymbolTypes()
}

func parents(f *ast.File) map[ast.Node]ast.Node {
	m := map[ast.Node]ast.Node{}
	var stack []ast.Node
	ast.Inspect(f, func(n ast.Node) bool {
		if n == nil {
			stack = stack[:len(stack)-1]
			return true
		}
		if len(stack) > 0 {
			m[n] = stack[len(stack)-1]
		}
		stack = append(stack, n)
		return true
	})
	return m
}

// origin is the generic declaration an instantiated function or field comes from.
func origin(obj types.Object) types.Object {
	switch o := obj.(type) {
	case *types.Func:
		return o.Origin()
	case *types.Var:
		return o.Origin()
	}
	return obj
}

// targetID is the id of what a name resolves to: a project declaration by its
// position, anything else by its package path and name, registered on first
// sight.
func (x *extractor) targetID(obj types.Object) (string, bool) {
	if obj == nil {
		return "", false
	}
	obj = origin(obj)
	if b, ok := obj.(*types.Builtin); ok && b.Pkg() != nil {
		// unsafe's functions are builtins with a package: `unsafe.Sizeof`.
		id := "ext:" + b.Pkg().Path() + "#" + b.Name()
		x.addExternal(id, b.Name(), "function", "external", b.Pkg().Path(), "")
		return id, true
	}
	if obj.Pkg() == nil {
		return x.universeID(obj)
	}
	if _, inRoot := x.byAbs[x.fileOf(obj.Pos())]; inRoot {
		return x.idOf(obj)
	}
	if x.isProjectPackage(obj.Pkg().Path()) {
		return "", false // declared in a file cgo generated
	}
	return x.externalID(obj)
}

// universeID names what Go predeclares: `lib#append`, `lib#error`. The basic
// types, `any`, `comparable`, `nil`, `true`, `false` and `iota` are written like
// keywords and are no references.
func (x *extractor) universeID(obj types.Object) (string, bool) {
	switch o := obj.(type) {
	case *types.Builtin:
		id := "lib#" + o.Name()
		x.addExternal(id, o.Name(), "function", "lib", "", "")
		return id, true
	case *types.TypeName:
		if o.Name() != "error" {
			return "", false
		}
		x.addExternal("lib#error", "error", "interface", "lib", "", "")
		x.extTypes["lib#error"] = o
		return "lib#error", true
	case *types.Func:
		x.addExternal("lib#error", "error", "interface", "lib", "", "")
		id := "lib#error." + o.Name()
		x.addExternal(id, o.Name(), "method", "lib", "", "lib#error")
		return id, true
	}
	return "", false
}

// externalID names a declaration outside the root: `ext:<import path>#Name`, and
// a member under its type, `ext:net/http#Server.Addr`.
func (x *extractor) externalID(obj types.Object) (string, bool) {
	pkg := obj.Pkg()
	module := pkg.Path()
	if m := x.meta[pkg.Path()]; m != nil && m.Module != nil {
		module = m.Module.Path
	}
	d, ok := describe(obj, nil)
	if !ok {
		return "", false
	}
	if obj.Parent() == pkg.Scope() {
		id := "ext:" + pkg.Path() + "#" + obj.Name()
		x.addExternal(id, obj.Name(), d.kind, "external", module, "")
		if tn, isType := obj.(*types.TypeName); isType && types.IsInterface(tn.Type()) {
			x.extTypes[id] = tn
		}
		return id, true
	}
	var ownerID string
	switch o := obj.(type) {
	case *types.Func:
		sig, _ := o.Type().(*types.Signature)
		if sig == nil || sig.Recv() == nil {
			return "", false
		}
		named := namedOf(sig.Recv().Type())
		if named == nil {
			return "", false
		}
		id, ok := x.targetID(named.Obj())
		if !ok {
			return "", false
		}
		ownerID = id
	case *types.Var:
		if !o.IsField() {
			return "", false
		}
		chain := x.fieldOwner(o)
		if chain == "" {
			return "", false
		}
		ownerID = x.externalOwner(pkg, module, chain)
		if ownerID == "" {
			return "", false
		}
	default:
		return "", false
	}
	id := ownerID + "." + obj.Name()
	x.addExternal(id, obj.Name(), d.kind, "external", module, ownerID)
	return id, true
}

func namedOf(t types.Type) *types.Named {
	t = types.Unalias(t)
	if p, ok := t.(*types.Pointer); ok {
		t = types.Unalias(p.Elem())
	}
	n, _ := t.(*types.Named)
	return n
}

// fieldOwner is the name path of the struct declaring an outside field — a
// types.Var does not know its struct, so a package's structs are indexed once.
func (x *extractor) fieldOwner(v *types.Var) string {
	if chain, ok := x.fieldOwners[v]; ok {
		return chain
	}
	pkg := v.Pkg()
	if x.ownersIndexed[pkg] {
		return ""
	}
	x.ownersIndexed[pkg] = true
	var index func(st *types.Struct, chain string)
	index = func(st *types.Struct, chain string) {
		for i := 0; i < st.NumFields(); i++ {
			f := st.Field(i)
			if _, seen := x.fieldOwners[f]; !seen {
				x.fieldOwners[f] = chain
			}
			if inner, ok := f.Type().(*types.Struct); ok {
				index(inner, chain+"."+f.Name())
			}
		}
	}
	scope := pkg.Scope()
	for _, name := range scope.Names() {
		if tn, ok := scope.Lookup(name).(*types.TypeName); ok && !tn.IsAlias() {
			if st, ok := tn.Type().Underlying().(*types.Struct); ok {
				index(st, name)
			}
		}
	}
	return x.fieldOwners[v]
}

func (x *extractor) externalOwner(pkg *types.Package, module, chain string) string {
	parts := strings.Split(chain, ".")
	id, ok := x.targetID(pkg.Scope().Lookup(parts[0]))
	if !ok {
		return ""
	}
	for _, p := range parts[1:] {
		child := id + "." + p
		x.addExternal(child, p, "property", "external", module, id)
		id = child
	}
	return id
}

func (x *extractor) callSiteID(c *ast.CallExpr) int {
	key := x.posKey(c.Lparen) + ":call"
	if id, ok := x.callSite[key]; ok {
		return id
	}
	id := x.nextCallSite
	x.nextCallSite++
	x.callSite[key] = id
	return id
}

func (w *refWalker) ident(n *ast.Ident) {
	x, info := w.x, w.info
	if n == w.s.file.Name || n.Name == "_" {
		return
	}
	obj, used := info.Uses[n]
	if !used {
		if _, declared := info.Defs[n]; !declared {
			w.unresolved(n)
		}
		return
	}
	switch o := obj.(type) {
	case *types.PkgName, *types.Label, *types.Nil:
		return
	case *types.Const:
		if o.Pkg() == nil {
			return
		}
	}
	id, ok := x.targetID(obj)
	if !ok {
		return
	}
	target := x.symbols[id]
	if target["origin"] == "project" {
		switch target["kind"] {
		case "local", "parameter", "type_parameter":
			return // the flow layer's
		}
	}
	role := w.role(n)
	_, isType := obj.(*types.TypeName)
	kind := "read"
	switch {
	case isType && role == "new":
		kind = "new"
	case isType && w.embeddedInInterface(n):
		kind = "extends"
	case isType:
		kind = "type"
	case role == "call" || role == "write" || role == "readwrite":
		kind = role
	case target["kind"] == "function" || target["kind"] == "method":
		kind = "value"
	}
	from := w.ownerOf(n)
	line := x.line(n.Pos())
	x.em.emit("ref", row{"from": from, "to": id, "kind": kind, "file": w.s.path, "line": line})
	if kind == "type" {
		x.em.emit("type_ref", row{"from": from, "to": id, "position": w.position(n), "file": w.s.path, "line": line})
	}
	if kind != "type" && kind != "new" && kind != "extends" {
		w.memberAccess(n, id, target, role, from, line)
	}
}

// ownerOf is the innermost enclosing declaration with a name — what a
// reference is from: a function or function literal, a type, a struct field or
// interface method, a package-level variable (for its initializer), else the
// file's <module>.
func (w *refWalker) ownerOf(n ast.Node) string {
	x, info := w.x, w.info
	prev := n
	for p := w.parent[n]; p != nil; prev, p = p, w.parent[p] {
		switch pp := p.(type) {
		case *ast.FuncLit:
			if id, ok := x.idByKey[x.posKey(pp.Pos())+":lit"]; ok {
				return id
			}
		case *ast.FuncDecl:
			if id, ok := x.idOf(info.Defs[pp.Name]); ok {
				return id
			}
		case *ast.TypeSpec:
			if id, ok := x.idOf(info.Defs[pp.Name]); ok {
				return id
			}
		case *ast.Field:
			if fl, ok := w.parent[pp].(*ast.FieldList); ok {
				switch w.parent[fl].(type) {
				case *ast.StructType, *ast.InterfaceType:
					if id, ok := w.fieldID(pp); ok {
						return id
					}
				}
			}
		case *ast.ValueSpec:
			if _, top := w.parent[w.parent[pp]].(*ast.File); !top {
				continue
			}
			i := 0
			for j, v := range pp.Values {
				if v == prev {
					i = j
				}
			}
			if i < len(pp.Names) {
				if id, ok := x.idOf(info.Defs[pp.Names[i]]); ok {
					return id
				}
			}
		}
	}
	return moduleID(w.s)
}

func (w *refWalker) fieldID(f *ast.Field) (string, bool) {
	if len(f.Names) > 0 {
		return w.x.idOf(w.info.Defs[f.Names[0]])
	}
	if ident := embeddedIdent(f.Type); ident != nil {
		if obj, ok := w.info.Defs[ident]; ok {
			return w.x.idOf(obj)
		}
	}
	return "", false
}

// role is how a name is used, judged at the expression it heads: `x.f`,
// `pkg.F`, `(f)`, `F[int]` are all their last name.
func (w *refWalker) role(n *ast.Ident) string {
	var r ast.Node = n
	_, instantiated := w.info.Instances[n]
climb:
	for {
		switch p := w.parent[r].(type) {
		case *ast.SelectorExpr:
			if p.Sel != r {
				break climb
			}
		case *ast.ParenExpr:
		case *ast.IndexExpr:
			if p.X != r || !instantiated {
				break climb
			}
		case *ast.IndexListExpr:
			if p.X != r || !instantiated {
				break climb
			}
		default:
			break climb
		}
		r = w.parent[r]
	}
	switch p := w.parent[r].(type) {
	case *ast.CallExpr:
		if p.Fun == r {
			if tv, ok := w.info.Types[p.Fun]; ok && tv.IsType() {
				return "convert"
			}
			return "call"
		}
	case *ast.CompositeLit:
		if p.Type == r {
			return "new"
		}
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
	case *ast.KeyValueExpr:
		if p.Key == r {
			if lit, ok := w.parent[p].(*ast.CompositeLit); ok {
				if tv, ok := w.info.Types[lit]; ok && tv.Type != nil {
					if _, isStruct := derefUnder(tv.Type).(*types.Struct); isStruct {
						return "write"
					}
				}
			}
		}
	case *ast.RangeStmt:
		if (p.Key == r || p.Value == r) && p.Tok == token.ASSIGN {
			return "write"
		}
	}
	return "read"
}

func derefUnder(t types.Type) types.Type {
	if p, ok := types.Unalias(t).(*types.Pointer); ok {
		t = p.Elem()
	}
	return t.Underlying()
}

func (w *refWalker) embeddedInInterface(n *ast.Ident) bool {
	var r ast.Node = n
	if sel, ok := w.parent[r].(*ast.SelectorExpr); ok && sel.Sel == n {
		r = sel
	}
	f, ok := w.parent[r].(*ast.Field)
	if !ok || len(f.Names) > 0 || f.Type != r {
		return false
	}
	fl, ok := w.parent[f].(*ast.FieldList)
	if !ok {
		return false
	}
	_, ok = w.parent[fl].(*ast.InterfaceType)
	return ok
}

// position is where a type is written, in `type_ref`'s vocabulary.
func (w *refWalker) position(n ast.Node) string {
	prev := n
	for p := w.parent[n]; p != nil; prev, p = p, w.parent[p] {
		switch pp := p.(type) {
		case *ast.FieldList:
			switch g := w.parent[pp].(type) {
			case *ast.FuncType:
				if g.Params == pp {
					return "param"
				}
				if g.Results == pp {
					return "return"
				}
				return "other" // type parameters
			case *ast.FuncDecl:
				return "param" // the receiver
			case *ast.StructType:
				return "property"
			case *ast.InterfaceType:
				return "extends"
			default:
				return "other"
			}
		case *ast.TypeAssertExpr:
			if pp.Type == prev {
				return "assertion"
			}
		case *ast.CaseClause:
			if _, ok := w.parent[w.parent[pp]].(*ast.TypeSwitchStmt); ok {
				return "assertion"
			}
			return "other"
		case *ast.CallExpr:
			if pp.Fun == prev {
				return "assertion" // a conversion
			}
			return "other"
		case *ast.IndexExpr:
			if pp.Index == prev {
				return "type_arg"
			}
		case *ast.IndexListExpr:
			for _, i := range pp.Indices {
				if i == prev {
					return "type_arg"
				}
			}
		case *ast.ValueSpec:
			if pp.Type == prev {
				return "variable"
			}
			return "other"
		case *ast.TypeSpec:
			if pp.Assign.IsValid() {
				return "alias"
			}
			return "other"
		case *ast.CompositeLit:
			return "other"
		case ast.Stmt, ast.Decl:
			return "other"
		}
	}
	return "other"
}

// memberAccess records a function touching a field or method of a project type.
func (w *refWalker) memberAccess(n *ast.Ident, id string, target row, role, from string, line int) {
	if target["origin"] != "project" {
		return
	}
	kind, _ := target["kind"].(string)
	if kind != "property" && kind != "method" {
		return
	}
	ownerID, _ := target["parent"].(string)
	owner := w.x.symbols[ownerID]
	if owner == nil {
		return
	}
	switch owner["kind"] {
	case "class", "interface", "type_alias":
	default:
		return
	}
	mode := "read"
	switch role {
	case "call":
		mode = "call"
	case "write", "readwrite":
		mode = role
	}
	viaThis := false
	if sel, ok := w.parent[n].(*ast.SelectorExpr); ok && sel.Sel == n {
		if recv, ok := ast.Unparen(sel.X).(*ast.Ident); ok {
			if v, ok := w.info.Uses[recv].(*types.Var); ok && v.Kind() == types.RecvVar {
				viaThis = true
			}
		}
	}
	w.x.em.emit("member_access", row{"fn": from, "member": id, "owner": ownerID, "mode": mode, "via_this": viaThis, "file": w.s.path, "line": line})
}

func calleeIdent(e ast.Expr) *ast.Ident {
	switch f := ast.Unparen(e).(type) {
	case *ast.Ident:
		return f
	case *ast.SelectorExpr:
		return f.Sel
	}
	return nil
}

// call records a call: a function or concrete method is `static`, a method of
// an interface (or of a type parameter's constraint) `virtual`, a project
// variable holding a function `indirect`. A function value computed in place
// (`wrap(r)(next)`) names no callee, so it is `unresolved`; points-to follows
// it through `callee_var`. A conversion `T(x)` is no call.
func (w *refWalker) call(c *ast.CallExpr) {
	x, info := w.x, w.info
	if tv, ok := info.Types[c.Fun]; ok && tv.IsType() {
		return
	}
	id := x.callSiteID(c)
	fun := ast.Unparen(c.Fun)
	switch f := fun.(type) {
	case *ast.IndexExpr:
		if ident := calleeIdent(f.X); ident != nil {
			if _, ok := info.Instances[ident]; ok {
				fun = ast.Unparen(f.X)
			}
		}
	case *ast.IndexListExpr:
		if ident := calleeIdent(f.X); ident != nil {
			if _, ok := info.Instances[ident]; ok {
				fun = ast.Unparen(f.X)
			}
		}
	}
	callee, dispatch, name := "", "unresolved", ""
	var ident *ast.Ident
	switch f := fun.(type) {
	case *ast.Ident:
		ident = f
	case *ast.SelectorExpr:
		ident = f.Sel
	case *ast.FuncLit:
		if lit, ok := x.idByKey[x.posKey(f.Pos())+":lit"]; ok {
			callee, dispatch = lit, "static"
		}
	}
	if ident != nil {
		name = ident.Name
		switch o := info.Uses[ident].(type) {
		case *types.Builtin, *types.Func:
			if cid, ok := x.targetID(o); ok {
				callee, dispatch = cid, "static"
				if fn, isFunc := o.(*types.Func); isFunc {
					if sig, _ := fn.Type().(*types.Signature); sig != nil && sig.Recv() != nil && types.IsInterface(sig.Recv().Type()) {
						dispatch = "virtual"
					}
				}
			}
		case *types.Var:
			if cid, ok := x.targetID(o); ok {
				callee = cid
				switch {
				case x.symbols[cid]["kind"] == "function":
					dispatch = "static" // `f := func() {…}`: f is that function
				case x.symbols[cid]["origin"] == "project":
					dispatch = "indirect"
				case o.IsField():
					dispatch = "virtual"
				default:
					dispatch = "static"
				}
			}
		}
	}
	x.em.emit("call_site", row{
		"id": id, "caller": w.ownerOf(c), "callee": nullable(callee), "callee_name": nullable(truncate(name, 100)),
		"dispatch": dispatch, "kind": "call", "file": w.s.path, "line": x.line(c.Pos()), "col": x.col(c.Pos()),
		"args": len(c.Args), "awaited": false, "optional": false, "spread": c.Ellipsis.IsValid(),
	})
}

// embedding records a struct's embedded types, and the methods the struct
// declares that hide one of an embedded type's.
func (w *refWalker) embedding(st *ast.StructType) {
	x, info := w.x, w.info
	for _, f := range st.Fields.List {
		if len(f.Names) > 0 {
			continue
		}
		ident := embeddedIdent(f.Type)
		if ident == nil {
			continue
		}
		tn, ok := info.Uses[ident].(*types.TypeName)
		if !ok {
			continue
		}
		innerID, ok := x.targetID(tn)
		if !ok {
			continue
		}
		outerID := w.ownerOf(f)
		switch o := x.objOf[outerID].(type) {
		case *types.TypeName:
		case *types.Var:
			if !o.IsField() {
				continue
			}
		default:
			continue
		}
		_, pointer := f.Type.(*ast.StarExpr)
		x.em.emit("embeds", row{"outer": outerID, "inner": innerID, "pointer": pointer})
		outer, isType := x.objOf[outerID].(*types.TypeName)
		if !isType {
			continue
		}
		named, ok := types.Unalias(outer.Type()).(*types.Named)
		if !ok {
			continue
		}
		innerType := selfInstance(tn)
		if innerType == nil {
			continue
		}
		inner := types.NewMethodSet(types.NewPointer(innerType))
		for i := 0; i < named.NumMethods(); i++ {
			m := named.Method(i)
			sel := inner.Lookup(m.Pkg(), m.Name())
			if sel == nil {
				continue
			}
			memberID, ok1 := x.targetID(m)
			baseID, ok2 := x.targetID(sel.Obj())
			if ok1 && ok2 && memberID != baseID {
				x.em.emit("overrides", row{"member": memberID, "base": baseID})
			}
		}
	}
}

// interfaceEmbedding records an interface embedding another as `extends`.
func (w *refWalker) interfaceEmbedding(it *ast.InterfaceType) {
	for _, f := range it.Methods.List {
		if len(f.Names) > 0 {
			continue
		}
		ident := embeddedIdent(f.Type)
		if ident == nil {
			continue // a union or `~T` in a constraint
		}
		tn, ok := w.info.Uses[ident].(*types.TypeName)
		if !ok || !types.IsInterface(tn.Type()) {
			continue
		}
		parentID, ok := w.x.targetID(tn)
		if !ok {
			continue
		}
		childID := w.ownerOf(f)
		if _, isType := w.x.objOf[childID].(*types.TypeName); !isType {
			continue
		}
		w.x.em.emit("extends", row{"child": childID, "parent": parentID})
	}
}

func (w *refWalker) unresolved(n *ast.Ident) {
	if sel, ok := w.parent[n].(*ast.SelectorExpr); ok && sel.Sel == n {
		// A member of something the checker could not type is the gap that
		// thing already is; a package's missing member is a gap of its own.
		qualifier := false
		if id, ok := ast.Unparen(sel.X).(*ast.Ident); ok {
			_, qualifier = w.info.Uses[id].(*types.PkgName)
		}
		if !qualifier && !typed(w.info, sel.X) {
			return
		}
	}
	if kv, ok := w.parent[n].(*ast.KeyValueExpr); ok && kv.Key == n {
		if lit, ok := w.parent[kv].(*ast.CompositeLit); ok && !typed(w.info, lit) {
			return
		}
	}
	kind := "read"
	switch {
	case w.role(n) == "call":
		kind = "call"
	case w.position(n) != "other":
		kind = "type"
	}
	w.x.em.emit("unresolved_ref", row{"from": w.ownerOf(n), "name": n.Name, "kind": kind, "file": w.s.path, "line": w.x.line(n.Pos())})
}

func typed(info *types.Info, e ast.Expr) bool {
	tv, ok := info.Types[e]
	return ok && tv.Type != nil && tv.Type != types.Typ[types.Invalid]
}

// selfInstance is a named type as its own methods see it: a generic type
// instantiated with its own type parameters, which is what a receiver `T[X]` is.
// Implements is unspecified for an uninstantiated generic type.
func selfInstance(tn *types.TypeName) types.Type {
	named, ok := types.Unalias(tn.Type()).(*types.Named)
	if !ok || named.TypeParams().Len() == 0 {
		return tn.Type()
	}
	args := make([]types.Type, named.TypeParams().Len())
	for i := range args {
		args[i] = named.TypeParams().At(i)
	}
	inst, err := types.Instantiate(nil, named, args, false)
	if err != nil {
		return nil
	}
	return inst
}

func isGeneric(tn *types.TypeName) bool {
	n, ok := types.Unalias(tn.Type()).(*types.Named)
	return ok && n.TypeParams().Len() > 0
}

// emitImplementations writes, for every named type the project declares, the
// interfaces it satisfies — each project interface, and each outside one the
// project names — and which method satisfies each of the interface's methods.
// The source says none of this; it is the type checker's answer, asked in a
// package view that sees both the type and the interface.
func (x *extractor) emitImplementations() {
	type iface struct {
		id    string
		obj   *types.TypeName
		names []string
	}
	var ifaces []iface
	add := func(id string, tn *types.TypeName) {
		if isGeneric(tn) {
			return
		}
		it, ok := tn.Type().Underlying().(*types.Interface)
		if !ok || it.NumMethods() == 0 || !it.IsMethodSet() {
			return
		}
		var names []string
		for i := 0; i < it.NumMethods(); i++ {
			names = append(names, it.Method(i).Name())
		}
		ifaces = append(ifaces, iface{id, tn, names})
	}
	for _, id := range x.symOrder {
		if x.symbols[id]["origin"] == "project" && x.symbols[id]["kind"] == "interface" {
			if tn, ok := x.objOf[id].(*types.TypeName); ok {
				add(id, tn)
			}
		}
	}
	var ext []string
	for id := range x.extTypes {
		ext = append(ext, id)
	}
	sort.Strings(ext)
	for _, id := range ext {
		add(id, x.extTypes[id])
	}
	if len(ifaces) == 0 {
		return
	}

	closures := map[*types.Package]map[string]*types.Package{}
	var views []*types.Package
	for _, s := range x.sources {
		v := s.pkg.Types
		if _, ok := closures[v]; ok {
			continue
		}
		c := map[string]*types.Package{}
		var visit func(p *types.Package)
		visit = func(p *types.Package) {
			if _, seen := c[p.Path()]; seen {
				return
			}
			c[p.Path()] = p
			for _, imp := range p.Imports() {
				visit(imp)
			}
		}
		visit(v)
		closures[v] = c
		views = append(views, v)
	}
	common := map[[2]string]map[string]*types.Package{}
	viewWith := func(a, b string) map[string]*types.Package {
		key := [2]string{a, b}
		if c, ok := common[key]; ok {
			return c
		}
		var found map[string]*types.Package
		for _, v := range views {
			if c := closures[v]; c[a] != nil && (b == "" || c[b] != nil) {
				found = c
				break
			}
		}
		common[key] = found
		return found
	}
	lookup := func(c map[string]*types.Package, tn *types.TypeName) *types.TypeName {
		if tn.Pkg() == nil {
			return tn
		}
		if c == nil || c[tn.Pkg().Path()] == nil {
			return nil
		}
		found, _ := c[tn.Pkg().Path()].Scope().Lookup(tn.Name()).(*types.TypeName)
		return found
	}
	isLocal := func(tn *types.TypeName) bool {
		return tn.Pkg() != nil && tn.Parent() != tn.Pkg().Scope()
	}

	for _, tid := range x.symOrder {
		if x.symbols[tid]["origin"] != "project" || x.symbols[tid]["kind"] != "class" {
			continue
		}
		tn, ok := x.objOf[tid].(*types.TypeName)
		if !ok || tn.Type().Underlying() == types.Typ[types.Invalid] {
			continue
		}
		self := selfInstance(tn)
		if self == nil {
			continue
		}
		has := map[string]bool{}
		ptrSet := types.NewMethodSet(types.NewPointer(self))
		for i := 0; i < ptrSet.Len(); i++ {
			has[ptrSet.At(i).Obj().Name()] = true
		}
		for _, in := range ifaces {
			covered := true
			for _, name := range in.names {
				if !has[name] {
					covered = false
					break
				}
			}
			if !covered {
				continue
			}
			var T, I *types.TypeName
			switch {
			case isLocal(tn) && isLocal(in.obj):
				if x.viewOf[tid] != x.viewOf[in.id] {
					continue
				}
				T, I = tn, in.obj
			case isLocal(tn):
				T, I = tn, lookup(closures[x.viewOf[tid]], in.obj)
			case isLocal(in.obj):
				T, I = lookup(closures[x.viewOf[in.id]], tn), in.obj
			default:
				ipath := ""
				if in.obj.Pkg() != nil {
					ipath = in.obj.Pkg().Path()
				}
				// The type's own view first: a test file's type exists only in the
				// view `go test` compiles, which has the package's import path.
				own := closures[x.viewOf[tid]]
				T, I = lookup(own, tn), lookup(own, in.obj)
				if T == nil || I == nil {
					c := viewWith(tn.Pkg().Path(), ipath)
					T, I = lookup(c, tn), lookup(c, in.obj)
				}
			}
			if T == nil || I == nil {
				continue
			}
			it, ok := I.Type().Underlying().(*types.Interface)
			if !ok {
				continue
			}
			t := selfInstance(T)
			if t == nil {
				continue
			}
			byValue := types.Implements(t, it)
			if !byValue && !types.Implements(types.NewPointer(t), it) {
				continue
			}
			x.em.emit("implements", row{"class": tid, "interface": in.id, "pointer": !byValue})
			ms := types.NewMethodSet(types.NewPointer(t))
			for i := 0; i < it.NumMethods(); i++ {
				m := it.Method(i)
				sel := ms.Lookup(m.Pkg(), m.Name())
				if sel == nil {
					continue
				}
				memberID, ok1 := x.targetID(sel.Obj())
				baseID, ok2 := x.targetID(m)
				if ok1 && ok2 && memberID != baseID {
					x.em.emit("overrides", row{"member": memberID, "base": baseID})
				}
			}
		}
	}
}

// emitSymbolTypes writes the type of every value-carrying project declaration,
// as go/types prints it, qualified by package name outside its own package.
func (x *extractor) emitSymbolTypes() {
	for _, id := range x.symOrder {
		if x.symbols[id]["origin"] != "project" {
			continue
		}
		var t types.Type
		switch o := x.objOf[id].(type) {
		case *types.Var, *types.Const, *types.Func:
			t = o.Type()
		case nil:
			t = x.litTypes[id]
		}
		if t == nil {
			continue
		}
		own := x.viewOf[id]
		qualify := func(p *types.Package) string {
			if own != nil && p.Path() == own.Path() {
				return ""
			}
			return p.Name()
		}
		_, isFunc := t.Underlying().(*types.Signature)
		// A type parameter's underlying type is its constraint, which says
		// nothing about what the value is.
		_, isTypeParam := types.Unalias(t).(*types.TypeParam)
		it, isIface := t.Underlying().(*types.Interface)
		x.em.emit("symbol_type", row{
			"symbol": id, "text": truncate(types.TypeString(t, qualify), 200),
			"is_any": !isTypeParam && isIface && it.Empty(), "is_unknown": false, "is_promise": false,
			"is_function": isFunc, "is_union": false,
		})
	}
}

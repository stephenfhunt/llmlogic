package main

import (
	"fmt"
	"go/ast"
	"go/token"
	"go/types"
	"os"
	"sort"
	"strconv"

	"golang.org/x/tools/go/ssa"
	"golang.org/x/tools/go/ssa/ssautil"
)

// The dataflow layer: three-address facts in the shape lib/pointsto.dl and
// lib/taint.dl read, lowered from x/tools' SSA form rather than from syntax —
// SSA has already made explicit what Go leaves implicit: dereferences, the
// variables a closure captures, the values of a multiple return, phis, and the
// conversions to and from interfaces.
//
// The model is the TypeScript layer's: flow-insensitive, field-based by name,
// context-insensitive. What Go adds:
//
//   - SSA values are registers, `<fn>$t3`; a named variable's symbol id receives
//     every value SSA reads or writes through it (its DebugRef instructions).
//   - A pointer to a struct or array is that object: its fields are fields of the
//     allocation, and copying the struct value copies the object. Any other
//     pointer's content is field `*`. A field or element address is resolved to
//     the object and field it names; an inline embedded struct's fields are the
//     outer object's.
//   - A closure is an allocation whose captured variables are fields `free0`, …,
//     loaded through the literal's own `$this`; a method's receiver is its
//     `this_var`, and a method call's receiver its `receiver`.
//   - Package-level variables are `cell` allocations in their file's <module>;
//     every project function is a `function` allocation there, so a function
//     value points to it.
//   - A channel's elements are field `<chan>`; `panic` stores into `$thrown` and
//     `recover()` reads it.
//
// Not modelled: a field address that escapes its function (`f(&s.x)`), the
// address of a package-level variable passed along, a method value's receiver
// (`g := x.M` then `g()`), and a promoted method reached through an embedded
// pointer.

type dfVar struct {
	fn, kind string
}

type dataflow struct {
	x          *extractor
	vars       map[string]dfVar
	varOrder   []string
	allocated  map[string]bool
	retDone    map[string]bool
	callExprs  map[string]bool
	nextSite   int
	thrownDecl bool
}

func (x *extractor) emitDataflow() {
	df := &dataflow{x: x, vars: map[string]dfVar{}, allocated: map[string]bool{}, retDone: map[string]bool{}, callExprs: map[string]bool{}, nextSite: 1}
	for _, s := range x.sources {
		ast.Inspect(s.file, func(n ast.Node) bool {
			if c, ok := n.(*ast.CallExpr); ok {
				df.callExprs[x.posKey(c.Lparen)+":call"] = true
			}
			return true
		})
	}

	prog, built := x.buildSSA()
	var fns []*ssa.Function
	for f := range ssautil.AllFunctions(prog) {
		if f.Blocks == nil || f.Origin() != nil || !built[f.Pkg] {
			continue
		}
		if f.Syntax() == nil && f.Synthetic != "package initializer" {
			continue
		}
		fns = append(fns, f)
	}
	sort.Slice(fns, func(i, j int) bool {
		pi, pj := x.fset.Position(fns[i].Pos()), x.fset.Position(fns[j].Pos())
		if pi.Filename != pj.Filename {
			return pi.Filename < pj.Filename
		}
		if pi.Offset != pj.Offset {
			return pi.Offset < pj.Offset
		}
		return fns[i].String() < fns[j].String()
	})

	// Every project function and package-level variable is an allocation in
	// its file's <module>, before any function reads it.
	for _, f := range fns {
		if id := x.functionID(f); id != "" && f.Syntax() != nil {
			df.declare(id, df.homeOf(id), "module")
			df.alloc(id, f.Pos(), "function", "", id, df.homeOf(id))
		}
	}
	for sp := range built {
		for _, m := range sp.Members {
			g, ok := m.(*ssa.Global)
			if !ok {
				continue
			}
			if id, ok := x.targetID(g.Object()); ok && x.symbols[id]["origin"] == "project" {
				df.declare(id, df.homeOf(id), "module")
				df.alloc(id, g.Pos(), "cell", "", "", df.homeOf(id))
			}
		}
	}

	done := map[string]bool{}
	for _, f := range fns {
		d := &dfn{df: df, x: x, f: f, addr: map[ssa.Value][2]string{}}
		if f.Synthetic == "package initializer" {
			d.perFile = true
		} else {
			d.id = x.functionID(f)
			if d.id == "" || done[d.id] {
				continue // a test variant's second copy of a function
			}
			done[d.id] = true
			d.home = d.id
			d.entry()
		}
		for _, b := range f.DomPreorder() {
			for _, in := range b.Instrs {
				if d.perFile && !d.attribute(in) {
					continue
				}
				d.lower(in)
			}
		}
	}
	for _, id := range df.varOrder {
		v := df.vars[id]
		x.em.emit("var", row{"id": id, "fn": v.fn, "kind": v.kind})
	}
}

// buildSSA creates an SSA package for every loaded package — from syntax where
// it type-checked, from type information otherwise, as for every dependency —
// and builds the ones with syntax. A package whose build fails is left out and
// said so on stderr; the rest of the extraction goes on.
func (x *extractor) buildSSA() (*ssa.Program, map[*ssa.Package]bool) {
	prog := ssa.NewProgram(x.fset, ssa.GlobalDebug)
	roots := map[*types.Package]bool{}
	for _, p := range x.roots {
		if p.Types != nil {
			roots[p.Types] = true
		}
	}
	deps := map[*types.Package]bool{}
	var visit func(tp *types.Package)
	visit = func(tp *types.Package) {
		for _, imp := range tp.Imports() {
			if roots[imp] || deps[imp] {
				continue
			}
			deps[imp] = true
			visit(imp)
		}
	}
	for tp := range roots {
		visit(tp)
	}
	var depList []*types.Package
	for tp := range deps {
		depList = append(depList, tp)
	}
	sort.Slice(depList, func(i, j int) bool { return depList[i].Path() < depList[j].Path() })
	for _, tp := range depList {
		prog.CreatePackage(tp, nil, nil, true)
	}
	built := map[*ssa.Package]bool{}
	for _, p := range x.roots {
		if p.Types == nil {
			continue
		}
		if p.IllTyped || p.TypesInfo == nil {
			prog.CreatePackage(p.Types, nil, nil, p.ForTest == "")
			continue
		}
		sp := prog.CreatePackage(p.Types, p.Syntax, p.TypesInfo, p.ForTest == "")
		built[sp] = true
	}
	for sp := range built {
		func() {
			defer func() {
				if r := recover(); r != nil {
					fmt.Fprintf(os.Stderr, "go-facts: no dataflow facts for %s: building its SSA form failed: %v\n", sp.Pkg.Path(), r)
					delete(built, sp)
				}
			}()
			sp.Build()
		}()
	}
	return prog, built
}

// functionID is the id of the project function an SSA function is: its
// declaration or literal, or — for a synthetic wrapper — the method it wraps.
func (x *extractor) functionID(f *ssa.Function) string {
	if o := f.Origin(); o != nil {
		f = o
	}
	switch syn := f.Syntax().(type) {
	case *ast.FuncDecl:
		if id, ok := x.idByKey[x.posKey(syn.Name.Pos())]; ok {
			return id
		}
	case *ast.FuncLit:
		if id, ok := x.idByKey[x.posKey(syn.Pos())+":lit"]; ok {
			return id
		}
	}
	if obj := f.Object(); obj != nil {
		if id, ok := x.targetID(obj); ok && x.symbols[id]["origin"] == "project" {
			return id
		}
	}
	return ""
}

func (df *dataflow) declare(id, fn, kind string) {
	if id == "" {
		return
	}
	if _, ok := df.vars[id]; ok {
		return
	}
	df.vars[id] = dfVar{fn, kind}
	df.varOrder = append(df.varOrder, id)
}

// homeOf is the <module> of the file declaring a project symbol.
func (df *dataflow) homeOf(id string) string {
	if file, ok := df.x.symbols[id]["file"].(string); ok {
		return file + "#<module>"
	}
	return ""
}

func (df *dataflow) alloc(v string, pos token.Pos, kind, typ, target, fallbackFn string) {
	if v == "" {
		return
	}
	key := v + "\x00" + df.x.posKey(pos) + "\x00" + kind
	if kind == "function" || kind == "cell" {
		if df.allocated[key] {
			return
		}
		df.allocated[key] = true
	}
	x := df.x
	file, line := "", 0
	if s, ok := x.byAbs[x.fileOf(pos)]; ok && pos.IsValid() {
		file, line = s.path, x.line(pos)
	} else if fallbackFn != "" {
		if f, ok := x.symbols[fallbackFn]["file"].(string); ok {
			file = f
			line, _ = x.symbols[fallbackFn]["line"].(int)
		} else if len(fallbackFn) > len("#<module>") {
			file = fallbackFn[:len(fallbackFn)-len("#<module>")]
			line = 1
		}
	}
	if file == "" {
		return
	}
	if line == 0 {
		line = 1
	}
	site := x.nextAllocSite()
	x.em.emit("alloc", row{"var": v, "site": site, "kind": kind, "type": nullable(typ), "fn_target": nullable(target), "file": file, "line": line})
}

func (x *extractor) nextAllocSite() int {
	x.allocSites++
	return x.allocSites
}

// dfn lowers one SSA function.
type dfn struct {
	df      *dataflow
	x       *extractor
	f       *ssa.Function
	id      string // the fn facts are attributed to
	home    string // the function's own id, for an allocation with no position
	perFile bool   // a package initializer: attributed to each instruction's file's <module>
	addr    map[ssa.Value][2]string
}

// attribute points a package initializer's instruction at its file's <module>;
// an instruction with no position keeps the last file seen.
func (d *dfn) attribute(in ssa.Instruction) bool {
	if pos := in.Pos(); pos.IsValid() {
		if s, ok := d.x.byAbs[d.x.fileOf(pos)]; ok {
			d.id = moduleID(s)
			d.home = d.id
		}
	}
	return d.id != ""
}

func (d *dfn) temp(name string) string {
	id := d.id + "$" + name
	d.df.declare(id, d.id, "temp")
	return id
}

func (d *dfn) this() string {
	id := d.id + "$this"
	d.df.declare(id, d.id, "this")
	return id
}

func (d *dfn) ret() string {
	id := d.id + "$ret"
	d.df.declare(id, d.id, "ret")
	if !d.df.retDone[id] {
		d.df.retDone[id] = true
		d.x.em.emit("formal_ret", row{"fn": d.id, "var": id})
	}
	return id
}

func (d *dfn) thrown() string {
	d.df.declare("$thrown", d.id, "thrown")
	return "$thrown"
}

func (d *dfn) freeVar(i int) string {
	return d.temp("free" + strconv.Itoa(i))
}

// localVar is the symbol id of a project local, parameter or result.
func (x *extractor) localVar(obj types.Object) (string, string, bool) {
	v, ok := obj.(*types.Var)
	if !ok || v.IsField() {
		return "", "", false
	}
	var id string
	if v.Name() == "" {
		id, ok = x.idByKey[x.posKey(v.Pos())+":param"]
	} else {
		id, ok = x.idOf(v)
	}
	if !ok {
		return "", "", false
	}
	r := x.symbols[id]
	switch r["kind"] {
	case "local":
		parent, _ := r["parent"].(string)
		return id, parent, true
	case "parameter":
		parent, _ := r["parent"].(string)
		return id, parent, true
	}
	return "", "", false
}

// vid is the variable a value lives in; "" for a value nothing tracks (a
// constant, a builtin, a function outside the root).
func (d *dfn) vid(v ssa.Value) string {
	x := d.x
	switch v := v.(type) {
	case nil, *ssa.Const, *ssa.Builtin:
		return ""
	case *ssa.Parameter:
		if id, parent, ok := x.localVar(v.Object()); ok {
			kind := "local"
			if x.symbols[id]["kind"] == "parameter" {
				kind = "param"
			}
			d.df.declare(id, parent, kind)
			return id
		}
		return d.temp(v.Name())
	case *ssa.Global:
		if id, ok := x.targetID(v.Object()); ok {
			d.df.declare(id, d.df.homeOf(id), "module")
			return id
		}
		return ""
	case *ssa.Function:
		if id := x.functionID(v); id != "" {
			d.df.declare(id, d.df.homeOf(id), "module")
			return id
		}
		return ""
	case *ssa.FreeVar:
		for i, fv := range d.f.FreeVars {
			if fv == v {
				return d.freeVar(i)
			}
		}
		return ""
	}
	if v.Name() == "" {
		return ""
	}
	return d.temp(v.Name())
}

func (d *dfn) assign(to, from, kind string) {
	if to == "" || from == "" || to == from {
		return
	}
	d.x.em.emit("assign", row{"to": to, "from": from, "kind": kind, "fn": d.id})
}

func (d *dfn) load(to, base, field string) {
	if to == "" || base == "" {
		return
	}
	d.x.em.emit("load", row{"to": to, "base": base, "field": field, "fn": d.id})
}

func (d *dfn) store(base, field, from string) {
	if base == "" || from == "" {
		return
	}
	d.x.em.emit("store", row{"base": base, "field": field, "from": from, "fn": d.id})
}

// entry writes a function's formals, its receiver, and its closure's captures.
func (d *dfn) entry() {
	x := d.x
	params := d.f.Params
	if d.f.Signature.Recv() != nil && len(params) > 0 {
		if recv := d.vid(params[0]); recv != "" {
			x.em.emit("this_var", row{"fn": d.id, "var": recv})
		}
		params = params[1:]
	}
	for i, p := range params {
		if v := d.vid(p); v != "" {
			x.em.emit("formal", row{"fn": d.id, "index": i, "var": v})
		}
	}
	if len(d.f.FreeVars) > 0 {
		this := d.this()
		x.em.emit("this_var", row{"fn": d.id, "var": this})
		for i := range d.f.FreeVars {
			d.load(d.freeVar(i), this, "free"+strconv.Itoa(i))
		}
	}
}

func pointee(t types.Type) types.Type {
	if p, ok := t.Underlying().(*types.Pointer); ok {
		return p.Elem()
	}
	return nil
}

func isAggregate(t types.Type) bool {
	if t == nil {
		return false
	}
	switch t.Underlying().(type) {
	case *types.Struct, *types.Array:
		return true
	}
	return false
}

func fieldName(t types.Type, i int) string {
	if p := pointee(t); p != nil {
		t = p
	}
	if st, ok := t.Underlying().(*types.Struct); ok && i < st.NumFields() {
		return st.Field(i).Name()
	}
	return "[]"
}

// object is the variable holding the object an address points into: an
// address of an inline struct or array field is still its outer object.
func (d *dfn) object(ptr ssa.Value) string {
	if a, ok := d.addr[ptr]; ok && isAggregate(pointee(ptr.Type())) {
		return a[0]
	}
	return d.vid(ptr)
}

// deref: `to = *ptr`.
func (d *dfn) deref(to string, ptr ssa.Value) {
	aggregate := isAggregate(pointee(ptr.Type()))
	if a, ok := d.addr[ptr]; ok {
		if aggregate {
			d.assign(to, a[0], "copy")
		} else {
			d.load(to, a[0], a[1])
		}
		return
	}
	if aggregate {
		d.assign(to, d.vid(ptr), "copy")
	} else {
		d.load(to, d.vid(ptr), "*")
	}
}

// storeThrough: `*ptr = from`.
func (d *dfn) storeThrough(ptr ssa.Value, from string) {
	aggregate := isAggregate(pointee(ptr.Type()))
	if a, ok := d.addr[ptr]; ok {
		if aggregate {
			d.assign(a[0], from, "copy")
		} else {
			d.store(a[0], a[1], from)
		}
		return
	}
	if aggregate {
		d.assign(d.vid(ptr), from, "copy")
	} else {
		d.store(d.vid(ptr), "*", from)
	}
}

func (d *dfn) lower(in ssa.Instruction) {
	x := d.x
	switch i := in.(type) {
	case *ssa.DebugRef:
		id, parent, ok := x.localVar(i.Object())
		if !ok {
			return
		}
		kind := "local"
		if x.symbols[id]["kind"] == "parameter" {
			kind = "param"
		}
		d.df.declare(id, parent, kind)
		d.vid(i.X) // declares the register, if it is one
		if i.IsAddr {
			if isAggregate(pointee(i.X.Type())) {
				d.assign(id, d.object(i.X), "copy")
			} else if a, ok := d.addr[i.X]; ok {
				d.load(id, a[0], a[1])
			} else {
				d.load(id, d.vid(i.X), "*")
			}
		} else {
			d.assign(id, d.vid(i.X), "copy")
		}
	case *ssa.Alloc:
		kind, typ := "cell", ""
		elem := pointee(i.Type())
		if elem != nil {
			switch elem.Underlying().(type) {
			case *types.Struct:
				kind = "object"
				if n := namedOf(elem); n != nil {
					if tid, ok := x.targetID(n.Obj()); ok {
						kind, typ = "instance", tid
					}
				}
			case *types.Array:
				kind = "array"
			}
		}
		d.df.alloc(d.vid(i), i.Pos(), kind, typ, "", d.home)
	case *ssa.MakeClosure:
		reg := d.vid(i)
		target := ""
		if fn, ok := i.Fn.(*ssa.Function); ok {
			target = x.functionID(fn)
		}
		d.df.alloc(reg, i.Pos(), "function", "", target, d.home)
		for k, b := range i.Bindings {
			d.store(reg, "free"+strconv.Itoa(k), d.vid(b))
		}
	case *ssa.MakeSlice:
		d.df.alloc(d.vid(i), i.Pos(), "slice", "", "", d.home)
	case *ssa.MakeMap:
		d.df.alloc(d.vid(i), i.Pos(), "map", "", "", d.home)
	case *ssa.MakeChan:
		d.df.alloc(d.vid(i), i.Pos(), "chan", "", "", d.home)
	case *ssa.MakeInterface:
		d.assign(d.vid(i), d.vid(i.X), "copy")
	case *ssa.ChangeType:
		d.assign(d.vid(i), d.vid(i.X), "copy")
	case *ssa.ChangeInterface:
		d.assign(d.vid(i), d.vid(i.X), "copy")
	case *ssa.SliceToArrayPointer:
		d.assign(d.vid(i), d.vid(i.X), "copy")
	case *ssa.TypeAssert:
		d.assign(d.vid(i), d.vid(i.X), "copy")
	case *ssa.Slice:
		d.assign(d.vid(i), d.object(i.X), "copy")
	case *ssa.Convert:
		kind := "derive"
		if pointee(i.Type()) != nil || pointee(i.X.Type()) != nil || isUnsafePointer(i.Type()) || isUnsafePointer(i.X.Type()) {
			kind = "copy"
		}
		d.assign(d.vid(i), d.vid(i.X), kind)
	case *ssa.MultiConvert:
		d.assign(d.vid(i), d.vid(i.X), "derive")
	case *ssa.Phi:
		reg := d.vid(i)
		for _, e := range i.Edges {
			d.assign(reg, d.vid(e), "copy")
		}
	case *ssa.Extract:
		d.assign(d.vid(i), d.vid(i.Tuple), "copy")
	case *ssa.FieldAddr:
		d.vid(i)
		d.addr[i] = [2]string{d.object(i.X), fieldName(i.X.Type(), i.Field)}
	case *ssa.IndexAddr:
		d.vid(i)
		if pointee(i.X.Type()) != nil {
			d.addr[i] = [2]string{d.object(i.X), "[]"}
		} else {
			d.addr[i] = [2]string{d.vid(i.X), "[]"} // an element of a slice
		}
	case *ssa.Field:
		d.load(d.vid(i), d.vid(i.X), fieldName(i.X.Type(), i.Field))
	case *ssa.Index:
		d.load(d.vid(i), d.vid(i.X), "[]")
	case *ssa.Lookup:
		d.load(d.vid(i), d.vid(i.X), "[]")
	case *ssa.MapUpdate:
		d.store(d.vid(i.Map), "[]", d.vid(i.Value))
	case *ssa.Range:
		d.assign(d.vid(i), d.vid(i.X), "copy")
	case *ssa.Next:
		d.load(d.vid(i), d.vid(i.Iter), "[]")
	case *ssa.UnOp:
		switch i.Op {
		case token.MUL:
			d.deref(d.vid(i), i.X)
		case token.ARROW:
			d.load(d.vid(i), d.vid(i.X), "<chan>")
		case token.NOT:
		default:
			d.assign(d.vid(i), d.vid(i.X), "derive")
		}
	case *ssa.BinOp:
		switch i.Op {
		case token.EQL, token.NEQ, token.LSS, token.LEQ, token.GTR, token.GEQ:
			return
		}
		reg := d.vid(i)
		d.assign(reg, d.vid(i.X), "derive")
		d.assign(reg, d.vid(i.Y), "derive")
	case *ssa.Store:
		d.storeThrough(i.Addr, d.vid(i.Val))
	case *ssa.Send:
		d.store(d.vid(i.Chan), "<chan>", d.vid(i.X))
	case *ssa.Select:
		tuple := d.vid(i)
		for _, st := range i.States {
			if st.Dir == types.SendOnly {
				d.store(d.vid(st.Chan), "<chan>", d.vid(st.Send))
			} else {
				d.load(tuple, d.vid(st.Chan), "<chan>")
			}
		}
	case *ssa.Return:
		for _, r := range i.Results {
			if v := d.vid(r); v != "" {
				d.assign(d.ret(), v, "copy")
			}
		}
	case *ssa.Panic:
		if v := d.vid(i.X); v != "" {
			d.assign(d.thrown(), v, "copy")
		}
	case *ssa.Call:
		d.call(i.Common(), i)
	case *ssa.Go:
		d.call(i.Common(), nil)
	case *ssa.Defer:
		d.call(i.Common(), nil)
	}
}

func isUnsafePointer(t types.Type) bool {
	b, ok := t.Underlying().(*types.Basic)
	return ok && b.Kind() == types.UnsafePointer
}

// call writes a call's callee value, receiver, arguments and result, under the
// call-site id of the call expression it came from. A call SSA made up (with
// no call expression in the source) has no call site, and no facts.
func (d *dfn) call(c *ssa.CallCommon, result *ssa.Call) {
	x := d.x
	key := x.posKey(c.Pos()) + ":call"
	if !c.Pos().IsValid() || !d.df.callExprs[key] {
		return
	}
	cs := x.callSiteAt(key)
	res := ""
	if result != nil {
		if t := result.Type(); t != nil {
			if tuple, ok := t.(*types.Tuple); !ok || tuple.Len() > 0 {
				res = d.vid(result)
			}
		}
	}
	args := c.Args
	switch {
	case c.IsInvoke():
		if r := d.vid(c.Value); r != "" {
			x.em.emit("receiver", row{"call_site": cs, "var": r})
		}
	default:
		switch fv := c.Value.(type) {
		case *ssa.Builtin:
			d.builtin(fv.Name(), args, res)
		case *ssa.Function:
			if v := d.vid(fv); v != "" {
				x.em.emit("callee_var", row{"call_site": cs, "var": v})
			}
			if fv.Signature.Recv() != nil && len(args) > 0 {
				if r := d.vid(args[0]); r != "" {
					x.em.emit("receiver", row{"call_site": cs, "var": r})
				}
				args = args[1:]
			}
		default:
			// A function value: the closure it holds is also the receiver, whose
			// fields are the variables the closure captured.
			if v := d.vid(c.Value); v != "" {
				x.em.emit("callee_var", row{"call_site": cs, "var": v})
				x.em.emit("receiver", row{"call_site": cs, "var": v})
			}
		}
	}
	for k, a := range args {
		if v := d.vid(a); v != "" {
			x.em.emit("actual", row{"call_site": cs, "index": k, "var": v})
		}
	}
	if res != "" {
		x.em.emit("actual_ret", row{"call_site": cs, "var": res})
	}
}

// builtin: `append` and `copy` move elements; `recover` reads what panicked.
func (d *dfn) builtin(name string, args []ssa.Value, res string) {
	switch name {
	case "append":
		if len(args) > 0 {
			d.assign(res, d.vid(args[0]), "copy")
		}
		if len(args) > 1 && res != "" {
			if src := d.vid(args[1]); src != "" {
				t := d.temp(fmt.Sprintf("append%d", d.x.nextAllocSite()))
				d.load(t, src, "[]")
				d.store(res, "[]", t)
			}
		}
	case "copy":
		if len(args) == 2 {
			if src, dst := d.vid(args[1]), d.vid(args[0]); src != "" && dst != "" {
				t := d.temp(fmt.Sprintf("copy%d", d.x.nextAllocSite()))
				d.load(t, src, "[]")
				d.store(dst, "[]", t)
			}
		}
	case "recover":
		d.assign(res, d.thrown(), "copy")
	}
}

// callSiteAt is the call-site id of a call expression, by the key the refs layer uses.
func (x *extractor) callSiteAt(key string) int {
	if id, ok := x.callSite[key]; ok {
		return id
	}
	id := x.nextCallSite
	x.nextCallSite++
	x.callSite[key] = id
	return id
}

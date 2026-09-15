package main

import (
	"go/ast"
	"go/parser"
	"go/scanner"
	"go/token"
	"go/types"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"unicode"
	"unicode/utf8"

	"golang.org/x/tools/go/packages"
)

var (
	generatedHeader = regexp.MustCompile(`(?i)@generated|auto-?generated|do not edit`)
	generatedPath   = regexp.MustCompile(`(^|/)__generated__/|\.gen\.[^/]+$|[._]generated\.[^/]+$|[._]pb2?(_grpc)?\.[^/]+$`)
)

func (x *extractor) emitStructure() {
	x.emitProjects()
	for _, s := range x.sources {
		head := s.text
		if len(head) > 600 {
			head = head[:600]
		}
		x.em.emit("file", row{
			"path": s.path, "dir": parentDir(s.path), "package": nullable(modulePath(s)), "lang": "go",
			"loc": lineCount(s.text), "sloc": sloc(s.text), "is_test": s.test, "is_decl": false,
			"is_generated": generatedHeader.Match(head) || generatedPath.MatchString(s.path),
			"namespace": s.pkg.Types.Path(),
		})
	}
	for _, e := range x.excluded {
		x.em.emit("excluded_file", row{"path": e.path, "reason": "build_constraint", "detail": nullable(e.detail)})
	}
	x.emitPackages()
	for _, s := range x.sources {
		x.emitImports(s)
		x.emitExports(s)
		x.emitDeclarationDetail(s)
	}
}

func (x *extractor) emitProjects() {
	for _, m := range x.modules {
		id := x.rel(m.gomod)
		var files []string
		for _, s := range x.sources {
			if s.module == m {
				files = append(files, s.path)
			}
		}
		x.em.emit("project", row{"id": id, "dir": x.rel(m.dir), "files": len(files), "strict": false, "module": nil, "target": nil})
		for _, f := range files {
			x.em.emit("project_file", row{"project": id, "file": f})
		}
	}
}

func (x *extractor) emitPackages() {
	for _, m := range x.modules {
		if m.path == "" {
			continue
		}
		x.em.emit("package", row{"name": m.path, "dir": x.rel(m.dir), "version": nil, "private": false})
		for _, r := range m.file.Require {
			kind, scope := "prod", "require"
			if r.Indirect {
				kind, scope = "optional", "indirect"
			}
			x.em.emit("package_dep", row{"package": m.path, "dep": r.Mod.Path, "kind": kind, "range": r.Mod.Version, "types_for": nil, "scope": scope})
		}
		for _, t := range m.file.Tool {
			x.em.emit("package_dep", row{"package": m.path, "dep": t.Path, "kind": "dev", "range": "", "types_for": nil, "scope": "tool"})
		}
		directive := func(name, path, version, replacement, replacementVersion string) {
			x.em.emit("module_directive", row{"package": m.path, "directive": name, "path": nullable(path), "version": nullable(version),
				"replacement": nullable(replacement), "replacement_version": nullable(replacementVersion)})
		}
		f := m.file
		if f.Go != nil {
			directive("go", "", f.Go.Version, "", "")
		}
		if f.Toolchain != nil {
			directive("toolchain", "", f.Toolchain.Name, "", "")
		}
		for _, r := range f.Replace {
			directive("replace", r.Old.Path, r.Old.Version, r.New.Path, r.New.Version)
		}
		for _, e := range f.Exclude {
			directive("exclude", e.Mod.Path, e.Mod.Version, "", "")
		}
		for _, r := range f.Retract {
			version := r.Low
			if r.High != r.Low {
				version = "[" + r.Low + ", " + r.High + "]"
			}
			directive("retract", "", version, "", "")
		}
		for _, g := range f.Godebug {
			directive("godebug", g.Key, g.Value, "", "")
		}
	}
}

// sloc counts lines carrying a token, comments excluded — the semicolons the
// scanner inserts at line ends are not tokens anyone wrote.
func sloc(text []byte) int {
	fset := token.NewFileSet()
	f := fset.AddFile("", -1, len(text))
	var sc scanner.Scanner
	sc.Init(f, text, func(token.Position, string) {}, 0)
	lines := map[int]bool{}
	for {
		pos, tok, lit := sc.Scan()
		if tok == token.EOF {
			break
		}
		if tok == token.SEMICOLON && lit == "\n" {
			continue
		}
		lines[f.Line(pos)] = true
	}
	return len(lines)
}

type firstUse struct {
	line int
	name string
}

// emitImports writes the file's import specs, one row per file of an in-root
// package the file references, and an `implicit` row per other file of its own
// package it references — the dependencies Go needs no statement for.
func (x *extractor) emitImports(s *source) {
	info := s.pkg.TypesInfo
	own := s.pkg.Types.Path()
	crossFiles := map[string][]string{} // import path → target files, in order of first use
	implicit := []string{}
	implicitAt := map[string]firstUse{}
	seenCross := map[string]bool{}
	ast.Inspect(s.file, func(n ast.Node) bool {
		ident, ok := n.(*ast.Ident)
		if !ok {
			return true
		}
		obj := info.Uses[ident]
		if obj == nil || obj.Pkg() == nil {
			return true
		}
		switch obj.(type) {
		case *types.PkgName, *types.Label, *types.Builtin, *types.Nil:
			return true
		}
		target, inRoot := x.byAbs[x.fileOf(obj.Pos())]
		if !inRoot || target == s {
			return true
		}
		path := obj.Pkg().Path()
		if path == own {
			if _, ok := implicitAt[target.path]; !ok {
				implicitAt[target.path] = firstUse{line: x.line(ident.Pos()), name: ident.Name}
				implicit = append(implicit, target.path)
			}
			return true
		}
		if k := path + "\x00" + target.path; !seenCross[k] {
			seenCross[k] = true
			crossFiles[path] = append(crossFiles[path], target.path)
		}
		return true
	})

	// The imports as written: cgo rewrites `import "C"` out of the file the type
	// checker sees, so the specs are read from the file on disk.
	wfset := token.NewFileSet()
	written, err := parser.ParseFile(wfset, s.abs, s.text, parser.ImportsOnly)
	if err != nil {
		written = s.file
		wfset = x.fset
	}
	for _, spec := range written.Imports {
		path, err := strconv.Unquote(spec.Path.Value)
		if err != nil {
			continue
		}
		line := wfset.Position(spec.Pos()).Line
		kind := "static"
		if spec.Name != nil {
			switch spec.Name.Name {
			case "_":
				kind = "side_effect"
			case ".":
				kind = "dot"
			}
		}
		base := row{"file": s.path, "line": line, "specifier": path, "kind": kind, "runtime": true,
			"target_file": nil, "target_package": nil, "target_ambient": nil, "unresolved_package": nil, "target_dir": nil}
		emit := func(extra row) {
			r := row{}
			for k, v := range base {
				r[k] = v
			}
			for k, v := range extra {
				r[k] = v
			}
			x.em.emit("imports", r)
		}
		if path == "C" {
			base["kind"] = "cgo"
			emit(row{"builtin": true, "target_package": "C", "resolved": true})
			continue
		}
		meta := x.meta[path]
		resolved := true
		switch {
		case x.isProjectPackage(path):
			dir := x.rel(packageDir(meta))
			base["target_dir"] = dir
			files := crossFiles[path]
			if len(files) == 0 {
				emit(row{"builtin": false, "resolved": true})
			}
			for _, f := range files {
				emit(row{"builtin": false, "resolved": true, "target_file": f})
			}
		case meta == nil || (len(meta.GoFiles) == 0 && len(meta.Errors) > 0):
			resolved = false
			emit(row{"builtin": false, "resolved": false})
		case meta.Module == nil && isStandard(path):
			emit(row{"builtin": true, "resolved": true, "target_package": path})
		default:
			pkg := path
			if meta.Module != nil {
				pkg = meta.Module.Path
			}
			emit(row{"builtin": false, "resolved": true, "target_package": pkg})
		}
		if kind != "side_effect" {
			local := path[strings.LastIndex(path, "/")+1:]
			if spec.Name != nil {
				local = spec.Name.Name
			} else if meta != nil && meta.Name != "" {
				local = meta.Name
			}
			target := ""
			if resolved {
				target = x.packageSymbol(path, meta)
			}
			x.em.emit("import_name", row{"file": s.path, "line": line, "local": local, "imported": "*",
				"target": nullable(target), "type_only": false})
		}
	}

	for _, f := range implicit {
		use := implicitAt[f]
		x.em.emit("imports", row{"file": s.path, "line": use.line, "specifier": use.name, "kind": "implicit", "runtime": true,
			"target_file": f, "target_package": nil, "target_ambient": nil, "builtin": false, "resolved": true,
			"unresolved_package": nil, "target_dir": parentDir(s.path)})
	}
}

func (x *extractor) isProjectPackage(path string) bool {
	_, ok := x.pkgSymbol[path]
	return ok
}

func packageDir(meta *packages.Package) string {
	if meta != nil && len(meta.GoFiles) > 0 {
		return filepath.Dir(meta.GoFiles[0])
	}
	return ""
}

// isStandard: the go command's own test — a standard-library path has no dot in
// its first element.
func isStandard(path string) bool {
	first, _, _ := strings.Cut(path, "/")
	return !strings.Contains(first, ".")
}

// packageSymbol is the id of an imported package: the project's own, or an
// external one registered on first sight.
func (x *extractor) packageSymbol(path string, meta *packages.Package) string {
	if id, ok := x.pkgSymbol[path]; ok {
		return id
	}
	if meta == nil {
		return ""
	}
	id := "ext:" + path + "#<package>"
	pkg := path
	if meta.Module != nil {
		pkg = meta.Module.Path
	}
	x.addExternal(id, meta.Name, "namespace", "external", pkg, "")
	return id
}

func (x *extractor) emitExports(s *source) {
	for _, decl := range s.file.Decls {
		var idents []*ast.Ident
		switch d := decl.(type) {
		case *ast.GenDecl:
			for _, spec := range d.Specs {
				switch sp := spec.(type) {
				case *ast.TypeSpec:
					idents = append(idents, sp.Name)
				case *ast.ValueSpec:
					idents = append(idents, sp.Names...)
				}
			}
		case *ast.FuncDecl:
			if d.Recv == nil {
				idents = append(idents, d.Name)
			}
		}
		for _, ident := range idents {
			obj := s.pkg.TypesInfo.Defs[ident]
			if obj == nil || !obj.Exported() {
				continue
			}
			id, ok := x.idOf(obj)
			if !ok {
				continue
			}
			_, isType := obj.(*types.TypeName)
			x.em.emit("exports", row{"file": s.path, "name": obj.Name(), "symbol": id, "kind": "local", "is_type": isType})
		}
	}
}

// emitDeclarationDetail writes each function-like's parameters, and the doc
// comment of every declaration above local scope.
func (x *extractor) emitDeclarationDetail(s *source) {
	info := s.pkg.TypesInfo
	x.emitDoc(moduleID(s), s.file.Doc)
	params := func(fnID string, t types.Type) {
		sig, ok := t.(*types.Signature)
		if !ok {
			return
		}
		for i := 0; i < sig.Params().Len(); i++ {
			p := sig.Params().At(i)
			id, ok := x.paramID(p)
			if !ok {
				continue
			}
			name := p.Name()
			if name == "" {
				name = "<unnamed>"
			}
			rest := sig.Variadic() && i == sig.Params().Len()-1
			x.em.emit("param", row{"fn": fnID, "index": i, "symbol": id, "name": name, "optional": false, "rest": rest, "has_default": false})
		}
	}
	ast.Inspect(s.file, func(n ast.Node) bool {
		switch n := n.(type) {
		case *ast.FuncDecl:
			if obj := info.Defs[n.Name]; obj != nil {
				if id, ok := x.idOf(obj); ok {
					params(id, obj.Type())
					x.emitDoc(id, n.Doc)
					if n.Recv == nil {
						x.emitEntryPoint(s, n, obj, id)
					}
				}
			}
		case *ast.StructType:
			x.emitFieldTags(s, n)
		case *ast.FuncLit:
			if id, ok := x.idByKey[x.posKey(n.Pos())+":lit"]; ok {
				if tv, ok := info.Types[n]; ok {
					params(id, tv.Type)
				}
			}
		case *ast.GenDecl:
			if n.Tok == token.IMPORT {
				return false
			}
			if n.Tok == token.CONST {
				x.emitTypedConsts(s, n)
			}
			for _, spec := range n.Specs {
				switch sp := spec.(type) {
				case *ast.TypeSpec:
					doc := sp.Doc
					if doc == nil && !n.Lparen.IsValid() {
						doc = n.Doc
					}
					if id, ok := x.idOf(info.Defs[sp.Name]); ok && x.isTopLevel(s, info.Defs[sp.Name]) {
						x.emitDoc(id, doc)
						x.emitMemberDocs(s, sp.Type)
					}
				case *ast.ValueSpec:
					doc := sp.Doc
					if doc == nil && !n.Lparen.IsValid() {
						doc = n.Doc
					}
					for _, name := range sp.Names {
						if obj := info.Defs[name]; obj != nil && x.isTopLevel(s, obj) {
							if id, ok := x.idOf(obj); ok {
								x.emitDoc(id, doc)
							}
						}
					}
				}
			}
		case *ast.InterfaceType:
			for _, f := range n.Methods.List {
				for _, name := range f.Names {
					if obj := info.Defs[name]; obj != nil {
						if id, ok := x.idOf(obj); ok {
							params(id, obj.Type())
						}
					}
				}
			}
		}
		return true
	})
}

// emitEntryPoint marks a function nothing in the code calls because the runtime
// or `go test` does, by the rules those apply: the name, the package, the file,
// and the signature.
func (x *extractor) emitEntryPoint(s *source, fd *ast.FuncDecl, obj types.Object, id string) {
	sig, ok := obj.Type().(*types.Signature)
	if !ok {
		return
	}
	name := fd.Name.Name
	bare := sig.Params().Len() == 0 && sig.Results().Len() == 0
	kind := ""
	switch {
	case name == "main" && s.pkg.Types.Name() == "main" && bare:
		kind = "main"
	case name == "init" && bare:
		kind = "init"
	case !s.test:
	case name == "TestMain" && takesTesting(sig, "M"):
		kind = "test_main"
	case testName(name, "Test") && takesTesting(sig, "T"):
		kind = "test"
	case testName(name, "Benchmark") && takesTesting(sig, "B"):
		kind = "benchmark"
	case testName(name, "Fuzz") && takesTesting(sig, "F"):
		kind = "fuzz"
	case testName(name, "Example") && bare:
		kind = "example"
	}
	if kind != "" {
		x.em.emit("entry_point", row{"symbol": id, "kind": kind})
	}
}

// testName is `go test`'s rule: the prefix alone, or the prefix followed by
// anything but a lower-case letter (`TestX`, `Test_x`, not `Testx`).
func testName(name, prefix string) bool {
	if !strings.HasPrefix(name, prefix) {
		return false
	}
	if len(name) == len(prefix) {
		return true
	}
	r, _ := utf8.DecodeRuneInString(name[len(prefix):])
	return !unicode.IsLower(r)
}

// takesTesting: a single `*testing.<name>` parameter and no result.
func takesTesting(sig *types.Signature, name string) bool {
	if sig.Params().Len() != 1 || sig.Results().Len() != 0 {
		return false
	}
	p, ok := types.Unalias(sig.Params().At(0).Type()).(*types.Pointer)
	if !ok {
		return false
	}
	n, ok := types.Unalias(p.Elem()).(*types.Named)
	return ok && n.Obj().Pkg() != nil && n.Obj().Pkg().Path() == "testing" && n.Obj().Name() == name
}

// emitFieldTags writes each tagged field's `key:"value"` pairs.
func (x *extractor) emitFieldTags(s *source, st *ast.StructType) {
	info := s.pkg.TypesInfo
	for _, f := range st.Fields.List {
		if f.Tag == nil {
			continue
		}
		raw, err := strconv.Unquote(f.Tag.Value)
		if err != nil {
			raw = f.Tag.Value
		}
		idents := f.Names
		if len(idents) == 0 {
			if ident := embeddedIdent(f.Type); ident != nil {
				idents = []*ast.Ident{ident}
			}
		}
		pairs, conventional := parseTag(raw)
		for _, name := range idents {
			id, ok := x.idOf(info.Defs[name])
			if !ok {
				continue
			}
			if !conventional {
				x.em.emit("field_tag", row{"field": id, "key": nil, "value": nil, "text": truncate(raw, 200)})
				continue
			}
			for _, p := range pairs {
				x.em.emit("field_tag", row{"field": id, "key": p[0], "value": truncate(p[1], 200), "text": truncate(raw, 200)})
			}
		}
	}
}

// parseTag splits a tag by the convention reflect.StructTag reads; false when
// the tag does not follow it.
func parseTag(tag string) ([][2]string, bool) {
	var out [][2]string
	for {
		i := 0
		for i < len(tag) && tag[i] == ' ' {
			i++
		}
		tag = tag[i:]
		if tag == "" {
			break
		}
		i = 0
		for i < len(tag) && tag[i] > ' ' && tag[i] != ':' && tag[i] != '"' && tag[i] != 0x7f {
			i++
		}
		if i == 0 || i+1 >= len(tag) || tag[i] != ':' || tag[i+1] != '"' {
			return nil, false
		}
		name := tag[:i]
		tag = tag[i+1:]
		i = 1
		for i < len(tag) && tag[i] != '"' {
			if tag[i] == '\\' {
				i++
			}
			i++
		}
		if i >= len(tag) {
			return nil, false
		}
		value, err := strconv.Unquote(tag[:i+1])
		if err != nil {
			return nil, false
		}
		out = append(out, [2]string{name, value})
		tag = tag[i+1:]
	}
	return out, len(out) > 0
}

// emitTypedConsts writes each constant of a named type, with its value and
// whether `iota` made it — a spec with no values repeats the one above.
func (x *extractor) emitTypedConsts(s *source, gd *ast.GenDecl) {
	info := s.pkg.TypesInfo
	var last []ast.Expr
	for _, spec := range gd.Specs {
		vs, ok := spec.(*ast.ValueSpec)
		if !ok {
			continue
		}
		values := vs.Values
		if len(values) == 0 {
			values = last
		} else {
			last = values
		}
		usesIota := false
		for _, v := range values {
			ast.Inspect(v, func(n ast.Node) bool {
				if id, ok := n.(*ast.Ident); ok {
					if c, ok := info.Uses[id].(*types.Const); ok && c.Pkg() == nil && c.Name() == "iota" {
						usesIota = true
					}
				}
				return !usesIota
			})
		}
		for _, name := range vs.Names {
			c, ok := info.Defs[name].(*types.Const)
			if !ok {
				continue
			}
			named, ok := types.Unalias(c.Type()).(*types.Named)
			if !ok {
				continue
			}
			id, ok := x.idOf(c)
			if !ok {
				continue
			}
			typeID, ok := x.targetID(named.Obj())
			if !ok {
				continue
			}
			value := ""
			if c.Val() != nil {
				value = truncate(c.Val().ExactString(), 100)
			}
			x.em.emit("typed_const", row{"symbol": id, "type": typeID, "value": nullable(value), "iota": usesIota})
		}
	}
}

func (x *extractor) isTopLevel(s *source, obj types.Object) bool {
	return obj != nil && obj.Pkg() != nil && obj.Parent() == obj.Pkg().Scope()
}

// emitMemberDocs documents a top-level type's fields and interface methods.
func (x *extractor) emitMemberDocs(s *source, t ast.Expr) {
	var fields []*ast.Field
	switch tt := t.(type) {
	case *ast.StructType:
		fields = tt.Fields.List
	case *ast.InterfaceType:
		fields = tt.Methods.List
	}
	for _, f := range fields {
		idents := f.Names
		if len(idents) == 0 {
			if ident := embeddedIdent(f.Type); ident != nil {
				idents = []*ast.Ident{ident}
			}
		}
		for _, name := range idents {
			if id, ok := x.idOf(s.pkg.TypesInfo.Defs[name]); ok {
				x.emitDoc(id, f.Doc)
			}
		}
	}
}

func (x *extractor) emitDoc(id string, doc *ast.CommentGroup) {
	if doc == nil {
		x.em.emit("doc", row{"symbol": id, "has_doc": false, "lines": 0})
		return
	}
	lines := x.line(doc.End()) - x.line(doc.Pos()) + 1
	x.em.emit("doc", row{"symbol": id, "has_doc": true, "lines": lines})
	if text := deprecation(doc.Text()); text != "" {
		x.em.emit("jsdoc_tag", row{"symbol": id, "tag": "deprecated", "text": truncate(text, 200)})
	}
}

// deprecation is the text of a doc comment's `Deprecated:` paragraph — Go's
// convention for what JSDoc spells `@deprecated`.
func deprecation(text string) string {
	for _, para := range strings.Split(text, "\n\n") {
		p := strings.TrimSpace(para)
		if rest, ok := strings.CutPrefix(p, "Deprecated:"); ok {
			if r := strings.Join(strings.Fields(rest), " "); r != "" {
				return r
			}
			return "deprecated"
		}
	}
	return ""
}

func truncate(s string, n int) string {
	one := strings.Join(strings.Fields(s), " ")
	if r := []rune(one); len(r) > n {
		return string(r[:n-1]) + "…"
	}
	return one
}

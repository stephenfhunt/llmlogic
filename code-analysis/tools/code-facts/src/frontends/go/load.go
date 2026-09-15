package main

import (
	"bufio"
	"bytes"
	"fmt"
	"go/ast"
	"go/token"
	"go/types"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"

	"golang.org/x/mod/modfile"
	"golang.org/x/tools/go/packages"
)

// A module is a go.mod the load reads as a main module: a project, and a package
// in `package`'s sense (the unit that declares dependencies).
type module struct {
	dir   string // absolute
	gomod string // absolute path of its go.mod
	path  string // module path
	file  *modfile.File
}

// A source is one .go file under the root, with the package view that
// type-checks it as its own: the plain package for a non-test file, the test
// variant (`p [p.test]` or `p_test [p.test]`) for a _test.go file.
type source struct {
	abs, path string
	pkg       *packages.Package
	file      *ast.File
	module    *module
	text      []byte
	test      bool
}

type excludedFile struct {
	path, detail string
}

type extractor struct {
	root     string
	layers   map[string]bool
	exclude  []*regexp.Regexp
	em       *emitter
	fset     *token.FileSet
	modules  []*module
	sources  []*source // in path order
	byAbs    map[string]*source
	meta     map[string]*packages.Package // import path → metadata (module, directory, files)
	roots    []*packages.Package          // every package view loaded, test variants included
	excluded []excludedFile

	idByKey   map[string]string
	keyByID   map[string]string
	symOrder  []string
	symbols   map[string]row
	pkgSymbol map[string]string // import path → the package's symbol id

	objOf    map[string]types.Object     // project id → the object declared (the first view's)
	viewOf   map[string]*types.Package   // project id → the package view that declared it
	litTypes map[string]types.Type       // an anonymous function's id → its signature
	extTypes map[string]*types.TypeName  // an interface outside the root that project code names
	callSite map[string]int              // call expression position → call-site id

	fieldOwners   map[*types.Var]string   // a field outside the root → its struct's name path
	ownersIndexed map[*types.Package]bool

	nextCallSite, nextFlowNode int
	allocSites                 int // the last allocation-site id handed out
}

func newExtractor(root string, layers map[string]bool, exclude []*regexp.Regexp, em *emitter) *extractor {
	return &extractor{
		root: root, layers: layers, exclude: exclude, em: em,
		fset:      token.NewFileSet(),
		byAbs:     map[string]*source{},
		meta:      map[string]*packages.Package{},
		idByKey:   map[string]string{},
		keyByID:   map[string]string{},
		symbols:   map[string]row{},
		pkgSymbol: map[string]string{},

		objOf:         map[string]types.Object{},
		viewOf:        map[string]*types.Package{},
		litTypes:      map[string]types.Type{},
		extTypes:      map[string]*types.TypeName{},
		callSite:      map[string]int{},
		fieldOwners:   map[*types.Var]string{},
		ownersIndexed: map[*types.Package]bool{},
	}
}

func (x *extractor) rel(abs string) string {
	r, err := filepath.Rel(x.root, abs)
	if err != nil {
		return filepath.ToSlash(abs)
	}
	return filepath.ToSlash(r)
}

func (x *extractor) underRoot(abs string) bool {
	return abs == x.root || strings.HasPrefix(abs, x.root+string(filepath.Separator))
}

// load reads every target — a go.mod or go.work, or a directory holding one —
// with the go command's own resolution.
func (x *extractor) load(targets []string) error {
	for _, t := range targets {
		abs, err := filepath.Abs(t)
		if err != nil {
			return err
		}
		dir, work := abs, ""
		if st, err := os.Stat(abs); err == nil && !st.IsDir() {
			dir = filepath.Dir(abs)
			if filepath.Base(abs) == "go.work" {
				work = abs
			}
		} else if fileExists(filepath.Join(abs, "go.work")) {
			work = filepath.Join(abs, "go.work")
		}
		var modDirs []string
		if work != "" {
			data, err := os.ReadFile(work)
			if err != nil {
				return err
			}
			wf, err := modfile.ParseWork(work, data, nil)
			if err != nil {
				return err
			}
			for _, u := range wf.Use {
				modDirs = append(modDirs, filepath.Join(dir, filepath.FromSlash(u.Path)))
			}
		} else {
			modDirs = []string{dir}
		}
		var patterns []string
		for _, md := range modDirs {
			m, err := readModule(md)
			if err != nil {
				return err
			}
			x.modules = append(x.modules, m)
			r, _ := filepath.Rel(dir, md)
			if r == "." {
				patterns = append(patterns, "./...")
			} else {
				patterns = append(patterns, "./"+filepath.ToSlash(r)+"/...")
			}
		}
		if err := x.loadPackages(dir, patterns); err != nil {
			return err
		}
	}
	sort.Slice(x.modules, func(i, j int) bool { return x.modules[i].gomod < x.modules[j].gomod })
	sort.Slice(x.sources, func(i, j int) bool { return x.sources[i].path < x.sources[j].path })
	sort.Slice(x.excluded, func(i, j int) bool { return x.excluded[i].path < x.excluded[j].path })
	return nil
}

func readModule(dir string) (*module, error) {
	gomod := filepath.Join(dir, "go.mod")
	data, err := os.ReadFile(gomod)
	if err != nil {
		return nil, err
	}
	f, err := modfile.Parse(gomod, data, nil)
	if err != nil {
		return nil, err
	}
	path := ""
	if f.Module != nil {
		path = f.Module.Mod.Path
	}
	return &module{dir: dir, gomod: gomod, path: path, file: f}, nil
}

func (x *extractor) loadPackages(dir string, patterns []string) error {
	// Metadata for every package reachable, dependencies included: which module
	// and directory an import path is. Cheap — no parsing.
	metaCfg := &packages.Config{
		Mode:  packages.NeedName | packages.NeedFiles | packages.NeedImports | packages.NeedDeps | packages.NeedModule,
		Dir:   dir,
		Tests: true,
	}
	metaPkgs, err := packages.Load(metaCfg, patterns...)
	if err != nil {
		return fmt.Errorf("listing packages in %s: %w", dir, err)
	}
	packages.Visit(metaPkgs, nil, func(p *packages.Package) {
		if p.ForTest == "" && !strings.HasSuffix(p.ID, ".test") {
			if _, seen := x.meta[p.PkgPath]; !seen {
				x.meta[p.PkgPath] = p
			}
		}
	})

	cfg := &packages.Config{
		Mode: packages.NeedName | packages.NeedFiles | packages.NeedCompiledGoFiles | packages.NeedImports |
			packages.NeedTypes | packages.NeedTypesSizes | packages.NeedSyntax | packages.NeedTypesInfo |
			packages.NeedModule | packages.NeedForTest,
		Dir:   dir,
		Tests: true,
		Fset:  x.fset,
	}
	pkgs, err := packages.Load(cfg, patterns...)
	if err != nil {
		return fmt.Errorf("loading packages in %s: %w", dir, err)
	}
	for _, p := range pkgs {
		if strings.HasSuffix(p.ID, ".test") {
			continue // the generated test main
		}
		x.roots = append(x.roots, p)
		for _, ignored := range p.IgnoredFiles {
			abs := filepath.Clean(ignored)
			if !strings.HasSuffix(abs, ".go") || !x.underRoot(abs) || x.excludedByUser(abs) {
				continue
			}
			x.addExcluded(abs)
		}
		for _, f := range p.Syntax {
			abs := x.fileOf(f.Package)
			if !x.underRoot(abs) || x.excludedByUser(abs) {
				continue
			}
			isTest := strings.HasSuffix(abs, "_test.go")
			if isTest != (p.ForTest != "") {
				continue // a non-test file seen again in a test variant, or a test file outside its own
			}
			if _, seen := x.byAbs[abs]; seen {
				continue
			}
			text, err := os.ReadFile(abs)
			if err != nil {
				return err
			}
			s := &source{abs: abs, path: x.rel(abs), pkg: p, file: f, module: x.moduleOf(abs), text: text, test: isTest}
			x.byAbs[abs] = s
			x.sources = append(x.sources, s)
		}
	}
	return nil
}

func (x *extractor) excludedByUser(abs string) bool {
	rel := x.rel(abs)
	for _, re := range x.exclude {
		if re.MatchString(rel) {
			return true
		}
	}
	return false
}

func (x *extractor) addExcluded(abs string) {
	path := x.rel(abs)
	for _, e := range x.excluded {
		if e.path == path {
			return
		}
	}
	x.excluded = append(x.excluded, excludedFile{path: path, detail: buildConstraint(abs)})
}

// buildConstraint is a file's `//go:build` line, if its header has one.
func buildConstraint(abs string) string {
	f, err := os.Open(abs)
	if err != nil {
		return ""
	}
	defer f.Close()
	sc := bufio.NewScanner(f)
	for sc.Scan() {
		line := bytes.TrimSpace(sc.Bytes())
		if bytes.HasPrefix(line, []byte("//go:build ")) {
			return string(line)
		}
		if bytes.HasPrefix(line, []byte("package ")) {
			break
		}
	}
	return ""
}

func (x *extractor) moduleOf(abs string) *module {
	var best *module
	for _, m := range x.modules {
		if (abs == m.dir || strings.HasPrefix(abs, m.dir+string(filepath.Separator))) && (best == nil || len(m.dir) > len(best.dir)) {
			best = m
		}
	}
	return best
}

func fileExists(p string) bool {
	st, err := os.Stat(p)
	return err == nil && !st.IsDir()
}

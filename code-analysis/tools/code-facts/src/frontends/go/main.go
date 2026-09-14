// Command go-facts is code-facts' Go frontend: a Go codebase as rows of
// code-facts' schema.
//
// Run by code-facts (Node), never alone. It writes one JSON object per line,
// {"relation": ..., "row": {...}}, and Node validates every row against
// src/schema.ts — the one home of the schema for every language — before
// writing anything. The last line is a `__counters__` row: the next free
// call-site and flow-node ids, where another frontend continues.
//
// Packages are loaded the way `go build` loads them (go/packages: modules,
// workspaces, build constraints, test variants) and type-checked by go/types,
// so every name in the facts is the compiler's resolution, not a guess.
package main

import (
	"bufio"
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"regexp"
	"strings"
)

type row = map[string]any

type emitter struct {
	enc *json.Encoder
}

func newEmitter(w *bufio.Writer) *emitter {
	enc := json.NewEncoder(w)
	enc.SetEscapeHTML(false)
	return &emitter{enc: enc}
}

func (e *emitter) emit(relation string, r row) {
	err := e.enc.Encode(struct {
		Relation string `json:"relation"`
		Row      row    `json:"row"`
	}{relation, r})
	if err != nil {
		fail(fmt.Errorf("encoding a %s row: %w", relation, err))
	}
}

type multiFlag []string

func (m *multiFlag) String() string     { return strings.Join(*m, ",") }
func (m *multiFlag) Set(v string) error { *m = append(*m, v); return nil }

func fail(err error) {
	fmt.Fprintln(os.Stderr, "go-facts:", err)
	os.Exit(1)
}

func main() {
	root := flag.String("root", "", "absolute path every file column is relative to")
	layers := flag.String("layers", "structure", "comma-separated layers to extract")
	firstCallSite := flag.Int("first-call-site", 1, "first call-site id")
	firstFlowNode := flag.Int("first-flow-node", 1, "first flow-node id")
	var exclude multiFlag
	flag.Var(&exclude, "exclude", "a regular expression over repo-relative paths to skip (repeatable)")
	flag.Parse()
	if *root == "" || flag.NArg() == 0 {
		fail(fmt.Errorf("usage: go-facts --root DIR [--layers L,...] [--exclude RE]... TARGET..."))
	}

	var excludes []*regexp.Regexp
	for _, e := range exclude {
		re, err := regexp.Compile(e)
		if err != nil {
			fail(fmt.Errorf("--exclude %q: %w", e, err))
		}
		excludes = append(excludes, re)
	}
	wanted := map[string]bool{}
	for _, l := range strings.Split(*layers, ",") {
		wanted[strings.TrimSpace(l)] = true
	}

	out := bufio.NewWriterSize(os.Stdout, 1<<20)
	x := newExtractor(*root, wanted, excludes, newEmitter(out))
	x.nextCallSite, x.nextFlowNode = *firstCallSite, *firstFlowNode
	if err := x.load(flag.Args()); err != nil {
		fail(err)
	}
	x.assignIDs()
	x.emitStructure()
	if wanted["refs"] {
		x.emitRefs()
	}
	x.flushSymbols()
	x.em.emit("__counters__", row{"call_site": x.nextCallSite, "flow_node": x.nextFlowNode})
	if err := out.Flush(); err != nil {
		fail(err)
	}
}

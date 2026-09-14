package main

import (
	_ "embed"
	"fmt"

	"example.com/shapes"
	g "example.com/shapes/internal/geom"
)

func main() {
	c := shapes.Circle{R: 2}
	var s shapes.Square
	fmt.Println(c.Area(), s.Area(), g.Hypot(3, 4))
}

var count int

var names []string

func tally(s *shapes.Square, c shapes.Circle) float64 {
	count++
	count = len(names)
	names = append(names, "c")
	return describe(s, c.Area)
}

func describe(sh shapes.Shape, area func() float64) float64 {
	return sh.Area() + area()
}

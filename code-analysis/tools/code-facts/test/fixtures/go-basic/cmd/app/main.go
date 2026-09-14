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

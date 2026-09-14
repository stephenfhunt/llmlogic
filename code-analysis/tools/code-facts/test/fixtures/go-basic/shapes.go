// Package shapes computes areas.
package shapes

import "math"

// Shape is anything with an area.
type Shape interface {
	Area() float64
}

// Circle is a round Shape.
type Circle struct {
	R float64
}

// Area of the circle.
func (c Circle) Area() float64 {
	return math.Pi * c.R * c.R
}

// Unit is the size every other is measured against.
const Unit = 1.0

var registry = map[string]Shape{}

// Register records a shape by name.
//
// Deprecated: keep a map of your own.
func Register(name string, s Shape) {
	registry[name] = s
}

// Map applies f to each element.
func Map[T any, U any](xs []T, f func(T) U) []U {
	out := make([]U, 0, len(xs))
	for _, x := range xs {
		out = append(out, f(x))
	}
	return out
}

// Sum adds its arguments.
func Sum(xs ...float64) (total float64) {
	for _, x := range xs {
		total += x
	}
	return total
}

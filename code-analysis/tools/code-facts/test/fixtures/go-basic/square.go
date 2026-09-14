package shapes

// Square has a name through its embedded base.
type Square struct {
	base
	Side float64
	meta struct {
		label string
	}
}

func (s *Square) Area() float64 {
	return s.Side * s.Side * Unit
}

var double = func(x float64) float64 { return 2 * x }

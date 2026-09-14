package shapes

type base struct {
	name string
}

func (b base) Name() string { return b.name }

// Celsius is a temperature.
type Celsius float64

// Round is another name for Circle.
type Round = Circle

type Namer interface {
	Name() string
	format(int, string) string
}

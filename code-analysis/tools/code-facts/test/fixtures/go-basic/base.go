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

// NamedShape is a Shape with a name.
type NamedShape interface {
	Shape
	Namer
}

// Error lets a base stand for an error.
func (b *base) Error() string { return b.Name() }

// Err is b as an error.
func (b *base) Err() error { return b }

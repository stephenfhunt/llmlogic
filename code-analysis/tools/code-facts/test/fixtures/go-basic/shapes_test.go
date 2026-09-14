package shapes

import "testing"

func TestArea(t *testing.T) {
	if (Circle{R: 1}).Area() <= 0 {
		t.Fatal("no area")
	}
}

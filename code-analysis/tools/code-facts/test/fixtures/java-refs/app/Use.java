package app;

import java.io.IOException;
import java.util.List;
import java.util.function.Supplier;
import shapes.Base;
import shapes.Circle;
import shapes.Plain;
import shapes.Shape;

public class Use {
  List<Shape> shapes;

  double total(List<Shape> items) {
    double sum = 0;
    for (Shape s : items) sum += s.area();
    items.forEach(s -> s.area());
    return sum;
  }

  Base make() throws IOException, IllegalStateException {
    Circle c = new Circle(2);
    c.radius = 3;
    new Plain();
    Supplier<Circle> fresh = Circle::new;
    Shape anon = new Shape() {
      @Override
      public double area() {
        return 0;
      }
    };
    if (anon instanceof Circle) return (Circle) anon;
    return Base.unit();
  }
}

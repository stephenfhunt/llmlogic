package shapes;

public class Circle extends Base {
  double radius;

  public Circle(double r) {
    super();
    radius = r;
  }

  public Circle() {
    this(1);
  }

  @Override
  public double area() {
    return Math.PI * radius * radius;
  }

  void grow() {
    this.radius += 1;
    scale = 2;
  }
}

package shapes;

public interface Shape {
  double area();

  default String name() {
    return "shape";
  }
}

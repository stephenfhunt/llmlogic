package shapes;

public abstract class Base implements Shape {
  protected double scale = 1;

  public abstract double area();

  public String describe() {
    return name() + area() * scale;
  }

  public static Base unit() {
    return new Circle(1);
  }

  @Override
  public String toString() {
    return describe();
  }
}

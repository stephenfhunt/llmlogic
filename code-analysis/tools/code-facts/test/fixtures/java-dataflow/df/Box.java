package df;

import java.util.function.UnaryOperator;

/** Fields, arrays, constructors, lambdas and method references. */
public class Box {
  static final Box SHARED = new Box(new Leaf("shared"));
  Object held;
  Object[] slots = new Object[4];

  Box(Object initial) {
    this.held = initial;
  }

  Object roundTrip(Object in) {
    this.held = in;
    Object out = this.held;
    slots[0] = out;
    return slots[1];
  }

  Object viaLocal() {
    Leaf leaf = new Leaf("local");
    Box box = new Box(leaf);
    return box.roundTrip(leaf);
  }

  static Object viaStatic() {
    Object s = SHARED.held;
    SHARED.held = s;
    return s;
  }

  Object throughLambda(Object seed) {
    UnaryOperator<Object> wrap = v -> new Leaf(String.valueOf(v));
    return wrap.apply(seed);
  }

  UnaryOperator<Object> reference() {
    return Box::identity;
  }

  static Object identity(Object v) {
    return v;
  }

  Object arrayOf(Object a, Object b) {
    Object[] both = {a, b};
    for (Object one : both) {
      held = one;
    }
    return both[0];
  }
}

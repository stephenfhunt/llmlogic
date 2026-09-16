package df;

/** Records, patterns, enums, switch expressions, try and catch. */
public final class Shapes {
  record Pair(Object left, Object right) {}

  record Nested(Pair inner, Object tail) {}

  enum Kind {
    ONE,
    TWO;
  }

  static Object component(Pair p) {
    return p.left();
  }

  static Object build(Object a, Object b) {
    Pair p = new Pair(a, b);
    return p.right();
  }

  static Object records() {
    Pair p = new Pair(new Leaf("l"), new Leaf("r"));
    return component(p);
  }

  static Object typePattern(Object v) {
    if (v instanceof Leaf leaf) {
      return leaf;
    }
    return null;
  }

  static Object recordPattern(Object v) {
    return switch (v) {
      case Nested(Pair(Object l, Object r), Object tail) -> l == null ? r : tail;
      case Leaf leaf -> leaf;
      default -> null;
    };
  }

  static Object enumConstant(Kind k) {
    switch (k) {
      case ONE:
        return Kind.ONE;
      default:
        return Kind.TWO;
    }
  }

  static Object caught(Object v) {
    Object held = null;
    try (AutoCloseable c = () -> {}) {
      held = v;
      throw new IllegalStateException("no");
    } catch (Exception e) {
      held = e;
    } finally {
      held = String.valueOf(held);
    }
    return held;
  }

  static int derived(int a, int b) {
    int n = a + b;
    n += a;
    return -n;
  }

  static Object ternary(Object a, Object b, boolean pick) {
    Object chosen = pick ? a : b;
    Object viaSwitch =
        switch (chosen == null ? 0 : 1) {
          case 0 -> a;
          default -> {
            yield b;
          }
        };
    return viaSwitch;
  }
}

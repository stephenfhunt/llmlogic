package flow;

import java.io.IOException;
import java.io.StringReader;
import java.util.List;
import java.util.function.IntSupplier;

public class Shapes {
  int total;

  int classify(int n) {
    if (n < 0) {
      return -1;
    } else if (n == 0 && total > 0) {
      return 0;
    }
    int sum = 0;
    for (int i = 0; i < n; i++) {
      sum += i;
    }
    while (sum > 10 || n > 100) {
      sum -= 10;
    }
    do {
      sum++;
    } while (sum < 3);
    return sum > 5 ? 1 : 2;
  }

  String name(int kind) {
    switch (kind) {
      case 0:
        total++;
      case 1:
        return "small";
      default:
        break;
    }
    String label = switch (kind) {
      case 2 -> "medium";
      case 3 -> {
        total += 3;
        yield "large";
      }
      default -> "other";
    };
    return label;
  }

  int read(List<String> lines) {
    int count = 0;
    outer:
    for (String line : lines) {
      for (char c : line.toCharArray()) {
        if (c == '#') continue outer;
        if (c == '!') break outer;
        count++;
      }
    }
    try (StringReader r = new StringReader("x")) {
      count += r.read();
    } catch (IOException e) {
      count = -1;
    } catch (RuntimeException e) {
      throw e;
    } finally {
      total = count;
    }
    synchronized (this) {
      total++;
    }
    return count;
  }

  IntSupplier later(int base) {
    int offset = 2;
    return () -> base + offset;
  }
}

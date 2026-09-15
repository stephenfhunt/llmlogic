package q;

import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CompletableFuture;

public class Types {
  List raw;
  Map.Entry<String, Integer> entry;
  Class<?> kind = List.class;

  Iterator legacy() {
    return null;
  }

  @Deprecated(since = "2")
  String cast(Object o) {
    List made = new ArrayList();
    boolean list = o instanceof List;
    List<String> typed = (List<String>) o;
    List untyped = (List) o;
    int n = (int) 3.5;
    var it = legacy();
    char c = 'x';
    long big = 0xFFL;
    String text = """
        hello
        """;
    return typed.get(0) + untyped.size() + made + c + big + text + n + list + it;
  }

  void fail(int code) throws Exception {
    try {
      if (code > 0) throw new IllegalStateException("bad");
      CompletableFuture.runAsync(() -> {});
      CompletableFuture<String> kept = CompletableFuture.completedFuture("kept");
      kept.join();
    } catch (IllegalStateException | IllegalArgumentException e) {
      throw e;
    } catch (RuntimeException e) {
    } catch (Exception e) {
      Runnable r = () -> {
        throw new RuntimeException(e);
      };
      r.run();
    }
  }

  record Pair(List left, String right) {}

  static class Legacy extends ArrayList {
    List<Map> nested;
    Object anonymous = new ArrayList() {};
  }
}

package codefacts;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * JSON with no library: rows are written with it, and the build's project model
 * is read with it. Objects are {@code Map<String, Object>} in insertion order,
 * arrays {@code List<Object>}, numbers {@code Long} or {@code Double}.
 */
final class Json {
  private Json() {}

  static String write(Object v) {
    StringBuilder b = new StringBuilder();
    write(b, v);
    return b.toString();
  }

  static void write(StringBuilder b, Object v) {
    if (v == null) {
      b.append("null");
    } else if (v instanceof String s) {
      string(b, s);
    } else if (v instanceof Boolean || v instanceof Integer || v instanceof Long) {
      b.append(v);
    } else if (v instanceof Number n) {
      b.append(n.doubleValue());
    } else if (v instanceof Map<?, ?> m) {
      b.append('{');
      boolean first = true;
      for (Map.Entry<?, ?> e : m.entrySet()) {
        if (!first) b.append(',');
        first = false;
        string(b, (String) e.getKey());
        b.append(':');
        write(b, e.getValue());
      }
      b.append('}');
    } else if (v instanceof Iterable<?> it) {
      b.append('[');
      boolean first = true;
      for (Object o : it) {
        if (!first) b.append(',');
        first = false;
        write(b, o);
      }
      b.append(']');
    } else {
      throw new IllegalArgumentException("no JSON form for " + v.getClass());
    }
  }

  private static void string(StringBuilder b, String s) {
    b.append('"');
    for (int i = 0; i < s.length(); i++) {
      char c = s.charAt(i);
      switch (c) {
        case '"' -> b.append("\\\"");
        case '\\' -> b.append("\\\\");
        case '\n' -> b.append("\\n");
        case '\r' -> b.append("\\r");
        case '\t' -> b.append("\\t");
        default -> {
          if (c < 0x20) b.append(String.format("\\u%04x", (int) c));
          else b.append(c);
        }
      }
    }
    b.append('"');
  }

  static Object parse(String text) {
    Parser p = new Parser(text);
    p.space();
    Object v = p.value();
    p.space();
    if (p.i != text.length()) throw p.error("trailing text");
    return v;
  }

  private static final class Parser {
    final String s;
    int i;

    Parser(String s) {
      this.s = s;
    }

    IllegalArgumentException error(String what) {
      return new IllegalArgumentException("JSON: " + what + " at offset " + i);
    }

    void space() {
      while (i < s.length() && Character.isWhitespace(s.charAt(i))) i++;
    }

    Object value() {
      if (i >= s.length()) throw error("unexpected end");
      char c = s.charAt(i);
      switch (c) {
        case '{': {
          i++;
          Map<String, Object> m = new LinkedHashMap<>();
          space();
          if (s.charAt(i) == '}') {
            i++;
            return m;
          }
          for (;;) {
            space();
            if (s.charAt(i) != '"') throw error("expected a key");
            String k = str();
            space();
            if (s.charAt(i++) != ':') throw error("expected ':'");
            space();
            m.put(k, value());
            space();
            char d = s.charAt(i++);
            if (d == '}') return m;
            if (d != ',') throw error("expected ',' or '}'");
          }
        }
        case '[': {
          i++;
          List<Object> l = new ArrayList<>();
          space();
          if (s.charAt(i) == ']') {
            i++;
            return l;
          }
          for (;;) {
            space();
            l.add(value());
            space();
            char d = s.charAt(i++);
            if (d == ']') return l;
            if (d != ',') throw error("expected ',' or ']'");
          }
        }
        case '"':
          return str();
        case 't':
          return word("true", Boolean.TRUE);
        case 'f':
          return word("false", Boolean.FALSE);
        case 'n':
          return word("null", null);
        default:
          return number();
      }
    }

    Object word(String w, Object v) {
      if (!s.startsWith(w, i)) throw error("expected " + w);
      i += w.length();
      return v;
    }

    Object number() {
      int start = i;
      while (i < s.length() && "+-0123456789.eE".indexOf(s.charAt(i)) >= 0) i++;
      String n = s.substring(start, i);
      if (n.isEmpty()) throw error("unexpected character");
      if (n.contains(".") || n.contains("e") || n.contains("E")) return Double.parseDouble(n);
      return Long.parseLong(n);
    }

    String str() {
      StringBuilder b = new StringBuilder();
      i++;
      for (;;) {
        char c = s.charAt(i++);
        if (c == '"') return b.toString();
        if (c != '\\') {
          b.append(c);
          continue;
        }
        char e = s.charAt(i++);
        switch (e) {
          case 'n' -> b.append('\n');
          case 'r' -> b.append('\r');
          case 't' -> b.append('\t');
          case 'b' -> b.append('\b');
          case 'f' -> b.append('\f');
          case 'u' -> {
            b.append((char) Integer.parseInt(s.substring(i, i + 4), 16));
            i += 4;
          }
          default -> b.append(e);
        }
      }
    }
  }
}

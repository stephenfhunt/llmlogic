package codefacts;

import java.util.ArrayList;
import java.util.BitSet;
import java.util.HashSet;
import java.util.Set;
import java.util.List;
import java.util.regex.Pattern;

/** What is read off a file's text rather than its trees. */
final class Text {
  private Text() {}

  static final Pattern GENERATED_HEADER = Pattern.compile("(?i)@generated|auto-?generated|do not edit");
  static final Pattern GENERATED_PATH = Pattern.compile("(^|/)__generated__/|\\.gen\\.[^/]+$|[._]generated\\.[^/]+$|[._]pb2?(_grpc)?\\.[^/]+$");

  static boolean generated(String path, String text) {
    String head = text.length() > 600 ? text.substring(0, 600) : text;
    return GENERATED_HEADER.matcher(head).find() || GENERATED_PATH.matcher(path).find();
  }

  static int loc(String text) {
    int n = 0;
    for (int i = 0; i < text.length(); i++) {
      if (text.charAt(i) == '\n') n++;
    }
    if (!text.isEmpty() && text.charAt(text.length() - 1) != '\n') n++;
    return n;
  }

  /** Lines carrying a token: not blank and not only comments. A literal marks every line it spans. */
  static int sloc(String t) {
    BitSet lines = new BitSet();
    int line = 1;
    int n = t.length();
    int i = 0;
    while (i < n) {
      char c = t.charAt(i);
      if (c == '\n') {
        line++;
        i++;
      } else if (Character.isWhitespace(c)) {
        i++;
      } else if (c == '/' && i + 1 < n && t.charAt(i + 1) == '/') {
        while (i < n && t.charAt(i) != '\n') i++;
      } else if (c == '/' && i + 1 < n && t.charAt(i + 1) == '*') {
        i += 2;
        while (i < n && !(t.charAt(i) == '*' && i + 1 < n && t.charAt(i + 1) == '/')) {
          if (t.charAt(i) == '\n') line++;
          i++;
        }
        i += 2;
      } else if (t.startsWith("\"\"\"", i)) {
        lines.set(line);
        i += 3;
        while (i < n && !t.startsWith("\"\"\"", i)) {
          char d = t.charAt(i);
          if (d == '\\' && i + 1 < n) {
            if (t.charAt(i + 1) == '\n') {
              line++;
              lines.set(line);
            }
            i += 2;
            continue;
          }
          if (d == '\n') {
            line++;
            lines.set(line);
          }
          i++;
        }
        i += 3;
      } else if (c == '"' || c == '\'') {
        lines.set(line);
        i++;
        while (i < n && t.charAt(i) != c && t.charAt(i) != '\n') {
          if (t.charAt(i) == '\\') i++;
          i++;
        }
        i++;
      } else {
        lines.set(line);
        i++;
      }
    }
    return lines.cardinality();
  }

  /** Skips whitespace and comments from {@code i}: the offset of the next token. */
  static int nextToken(String t, int i) {
    int n = t.length();
    while (i < n) {
      char c = t.charAt(i);
      if (Character.isWhitespace(c)) {
        i++;
      } else if (c == '/' && i + 1 < n && t.charAt(i + 1) == '/') {
        while (i < n && t.charAt(i) != '\n') i++;
      } else if (c == '/' && i + 1 < n && t.charAt(i + 1) == '*') {
        int end = t.indexOf("*/", i + 2);
        i = end < 0 ? n : end + 2;
      } else {
        break;
      }
    }
    return i;
  }

  /** The Javadoc comment just before offset {@code start}, as {@code [begin, end)}, or null. */
  static int[] docBefore(String t, int start) {
    int i = Math.min(start, t.length()) - 1;
    while (i >= 0 && Character.isWhitespace(t.charAt(i))) i--;
    if (i < 1 || t.charAt(i) != '/' || t.charAt(i - 1) != '*') return null;
    int open = t.lastIndexOf("/*", i - 2);
    if (open < 0 || !t.startsWith("/**", open) || open + 3 > i - 1) return null;
    return new int[] {open, i + 1};
  }

  record Tag(String name, String text) {}

  /** A Javadoc comment's block tags, `@name text`, each with the lines that continue it. */
  static List<Tag> blockTags(String comment) {
    String body = comment.substring(3, comment.length() - 2);
    List<Tag> out = new ArrayList<>();
    String name = null;
    StringBuilder text = new StringBuilder();
    for (String raw : body.split("\n", -1)) {
      String l = raw.strip();
      if (l.startsWith("*")) l = l.substring(1).strip();
      if (l.length() > 1 && l.charAt(0) == '@' && Character.isLetter(l.charAt(1))) {
        if (name != null) out.add(tag(name, text));
        int sp = 1;
        while (sp < l.length() && !Character.isWhitespace(l.charAt(sp))) sp++;
        name = l.substring(1, sp);
        text = new StringBuilder(l.substring(sp).strip());
      } else if (name != null && !l.isEmpty()) {
        if (text.length() > 0) text.append(' ');
        text.append(l);
      }
    }
    if (name != null) out.add(tag(name, text));
    return out;
  }

  private static Tag tag(String name, StringBuilder text) {
    String s = text.toString().strip();
    return new Tag(name, s.isEmpty() ? null : truncate(s, 200));
  }

  record Halstead(int operators, int operands, int distinctOperators, int distinctOperands) {}

  private static final Set<String> KEYWORDS = Set.of(
      "abstract", "assert", "boolean", "break", "byte", "case", "catch", "char", "class", "const", "continue", "default", "do", "double",
      "else", "enum", "extends", "final", "finally", "float", "for", "goto", "if", "implements", "import", "instanceof", "int", "interface",
      "long", "native", "new", "package", "private", "protected", "public", "return", "short", "static", "strictfp", "super", "switch",
      "synchronized", "throw", "throws", "transient", "try", "void", "volatile", "while");
  private static final List<String> OPERATORS = List.of(
      ">>>=", "<<=", ">>=", ">>>", "...", "->", "::", "++", "--", "&&", "||", "==", "!=", "<=", ">=", "+=", "-=", "*=", "/=", "&=", "|=",
      "^=", "%=", "<<", ">>");

  /**
   * Halstead's counts over `[start, end)` of a Java source, `skip` ranges left out:
   * identifiers, literals, `true`, `false`, `null` and `this` are operands; keywords,
   * operators and punctuation operators. javac's own tokenizer is not public API,
   * so this reads the text, as `sloc` does.
   */
  static Halstead halstead(String t, int start, int end, List<int[]> skip) {
    int operators = 0;
    int operands = 0;
    Set<String> distinctOperators = new HashSet<>();
    Set<String> distinctOperands = new HashSet<>();
    int i = start;
    outer:
    while (i < end) {
      for (int[] r : skip) {
        if (i >= r[0] && i < r[1]) {
          i = r[1];
          continue outer;
        }
      }
      char c = t.charAt(i);
      if (Character.isWhitespace(c)) {
        i++;
      } else if (c == '/' && i + 1 < end && t.charAt(i + 1) == '/') {
        while (i < end && t.charAt(i) != '\n') i++;
      } else if (c == '/' && i + 1 < end && t.charAt(i + 1) == '*') {
        int close = t.indexOf("*/", i + 2);
        i = close < 0 ? end : close + 2;
      } else if (t.startsWith("\"\"\"", i)) {
        int close = t.indexOf("\"\"\"", i + 3);
        int stop = close < 0 ? end : close + 3;
        operands++;
        distinctOperands.add(t.substring(i, Math.min(stop, end)));
        i = stop;
      } else if (c == '"' || c == '\'') {
        int j = i + 1;
        while (j < end && t.charAt(j) != c && t.charAt(j) != '\n') {
          if (t.charAt(j) == '\\') j++;
          j++;
        }
        operands++;
        distinctOperands.add(t.substring(i, Math.min(j + 1, end)));
        i = j + 1;
      } else if (Character.isDigit(c) || c == '.' && i + 1 < end && Character.isDigit(t.charAt(i + 1))) {
        int j = i;
        while (j < end && (Character.isLetterOrDigit(t.charAt(j)) || t.charAt(j) == '.' || t.charAt(j) == '_')) j++;
        operands++;
        distinctOperands.add(t.substring(i, j));
        i = j;
      } else if (Character.isJavaIdentifierStart(c)) {
        int j = i;
        while (j < end && Character.isJavaIdentifierPart(t.charAt(j))) j++;
        String word = t.substring(i, j);
        if (KEYWORDS.contains(word)) {
          operators++;
          distinctOperators.add(word);
        } else {
          operands++;
          distinctOperands.add(word);
        }
        i = j;
      } else {
        String op = String.valueOf(c);
        for (String o : OPERATORS) {
          if (t.startsWith(o, i)) {
            op = o;
            break;
          }
        }
        operators++;
        distinctOperators.add(op);
        i += op.length();
      }
    }
    return new Halstead(operators, operands, distinctOperators.size(), distinctOperands.size());
  }

  static String truncate(String s, int n) {
    return s.length() <= n ? s : s.substring(0, n);
  }
}

package df;

/** A plain class the other fixtures allocate. */
public class Leaf {
  final String tag;

  Leaf(String tag) {
    this.tag = tag;
  }

  String tag() {
    return tag;
  }
}

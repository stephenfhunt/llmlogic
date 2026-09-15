package com.example.app;

import static com.example.core.Kind.*;
import static com.example.core.geom.Geom.sq;

import com.example.core.Circle;
import com.example.core.Tag;
import com.example.core.geom.*;
import java.util.List;

public class Main {
  public static void main(String[] args) {
    List<Circle> circles = List.of(new Circle(sq(2)));
    Point p = new Point(1, 2);
    System.out.println(circles.size() + p.x() + ROUND.ordinal());
    System.out.println(com.example.core.Registry.create());
    System.out.println(Geom.TAU);
  }
}

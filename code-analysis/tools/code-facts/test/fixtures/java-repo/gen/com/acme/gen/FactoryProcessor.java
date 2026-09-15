package com.acme.gen;

import java.io.IOException;
import java.io.Writer;
import java.util.Set;
import javax.annotation.processing.AbstractProcessor;
import javax.annotation.processing.RoundEnvironment;
import javax.annotation.processing.SupportedAnnotationTypes;
import javax.lang.model.SourceVersion;
import javax.lang.model.element.Element;
import javax.lang.model.element.TypeElement;
import javax.tools.Diagnostic;

/** Generates `<Name>Factory` with a static `create()` for each class annotated `@Factory`. */
@SupportedAnnotationTypes("com.acme.gen.Factory")
public final class FactoryProcessor extends AbstractProcessor {
  @Override
  public SourceVersion getSupportedSourceVersion() {
    return SourceVersion.latestSupported();
  }

  @Override
  public boolean process(Set<? extends TypeElement> annotations, RoundEnvironment env) {
    for (Element e : env.getElementsAnnotatedWith(Factory.class)) {
      TypeElement type = (TypeElement) e;
      String pkg = processingEnv.getElementUtils().getPackageOf(type).getQualifiedName().toString();
      String name = type.getSimpleName() + "Factory";
      try (Writer w = processingEnv.getFiler().createSourceFile(pkg + "." + name, type).openWriter()) {
        w.write("package " + pkg + ";\n\npublic final class " + name + " {\n  public static " + type.getSimpleName() + " create() {\n    return new "
            + type.getSimpleName() + "();\n  }\n}\n");
      } catch (IOException ex) {
        processingEnv.getMessager().printMessage(Diagnostic.Kind.ERROR, ex.toString(), type);
      }
    }
    return true;
  }
}

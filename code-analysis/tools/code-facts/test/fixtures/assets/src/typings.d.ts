// How a project types an import of something that is not a module: a wildcard
// ambient declaration. The specifier then resolves — the checker knows it — to
// no file and no package, which is what `imports.target_ambient` records.
declare module "*.css" {}
declare module "bundler!*" {}

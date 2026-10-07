Add triangles to `shapes.lot`:

- a variant `Triangle(a: f64, b: f64, c: f64)` of `Shape`, given by its three side lengths;
- its area by Heron's formula, its perimeter, and `name` returning `"triangle"`;
- a new `fn valid(s: Shape) -> bool`: every length or radius is above zero and, for a triangle, each side is shorter than the sum of the other two.

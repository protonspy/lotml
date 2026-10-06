#include <stdint.h>
#include <stdio.h>

static int64_t mandelbrot(int64_t size, int64_t limit) {
    int64_t inside = 0;
    for (int64_t y = 0; y < size; y++) {
        double ci = 2.0 * (double)y / (double)size - 1.0;
        for (int64_t x = 0; x < size; x++) {
            double cr = 2.0 * (double)x / (double)size - 1.5;
            double zr = 0.0, zi = 0.0;
            int64_t i = 0;
            while (i < limit && zr * zr + zi * zi <= 4.0) {
                double t = zr * zr - zi * zi + cr;
                zi = 2.0 * zr * zi + ci;
                zr = t;
                i++;
            }
            if (i == limit) inside++;
        }
    }
    return inside;
}

int main(void) {
    printf("%lld\n", (long long)mandelbrot(800, 300));
    return 0;
}

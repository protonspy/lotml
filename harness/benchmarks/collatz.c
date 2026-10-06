#include <stdint.h>
#include <stdio.h>

int main(void) {
    int64_t best = 0, best_n = 0;
    for (int64_t n = 1; n < 1500000; n++) {
        int64_t x = n, steps = 0;
        while (x != 1) {
            if (x % 2 == 0) {
                x = x / 2;
            } else {
                x = 3 * x + 1;
            }
            steps++;
        }
        if (steps > best) {
            best = steps;
            best_n = n;
        }
    }
    printf("(%lld, %lld)\n", (long long)best_n, (long long)best);
    return 0;
}

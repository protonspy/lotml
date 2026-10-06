#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int main(void) {
    int64_t n = 40000;
    int64_t *base = malloc((size_t)n * sizeof(int64_t));
    if (base == NULL) abort();
    for (int64_t i = 0; i < n; i++) base[i] = i;
    int64_t total = 0;
    for (int64_t i = 0; i < 20000; i++) {
        int64_t *copy = malloc((size_t)n * sizeof(int64_t));
        if (copy == NULL) abort();
        memcpy(copy, base, (size_t)n * sizeof(int64_t));
        copy[i] = -1;
        total += copy[i] + copy[i + 1] + base[i];
        free(copy);
    }
    printf("%lld\n", (long long)total);
    free(base);
    return 0;
}

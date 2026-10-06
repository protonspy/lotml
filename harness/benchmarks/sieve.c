#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

static int64_t sieve(int64_t n) {
    bool *flags = malloc((size_t)(n + 1));
    if (flags == NULL) abort();
    for (int64_t k = 0; k <= n; k++) flags[k] = true;
    flags[0] = false;
    flags[1] = false;
    for (int64_t i = 2; i * i <= n; i++) {
        if (flags[i]) {
            for (int64_t j = i * i; j <= n; j += i) flags[j] = false;
        }
    }
    int64_t count = 0;
    for (int64_t k = 0; k <= n; k++) {
        if (flags[k]) count++;
    }
    free(flags);
    return count;
}

int main(void) {
    int64_t total = 0;
    for (int k = 0; k < 20; k++) total += sieve(4000000);
    printf("%lld\n", (long long)total);
    return 0;
}

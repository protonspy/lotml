#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

static double *matrix(int64_t n) {
    double *m = calloc((size_t)(n * n), sizeof(double));
    if (m == NULL) abort();
    return m;
}

int main(void) {
    int64_t n = 700;
    double *a = matrix(n), *b = matrix(n), *c = matrix(n);
    for (int64_t i = 0; i < n; i++) {
        for (int64_t j = 0; j < n; j++) {
            a[i * n + j] = (double)(i * n + j) / (double)(n * n);
            b[i * n + j] = (double)(i - j) / (double)n;
        }
    }
    for (int64_t i = 0; i < n; i++) {
        double *row = &c[i * n];
        for (int64_t k = 0; k < n; k++) {
            double aik = a[i * n + k];
            const double *bk = &b[k * n];
            for (int64_t j = 0; j < n; j++) row[j] += aik * bk[j];
        }
    }
    double total = 0.0;
    for (int64_t i = 0; i < n * n; i++) total += c[i];
    printf("%lld\n", (long long)(total * 1000.0));
    free(a);
    free(b);
    free(c);
    return 0;
}

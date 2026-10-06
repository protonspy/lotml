#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int main(void) {
    int64_t n = 600000;
    char **parts = malloc((size_t)n * sizeof(char *));
    if (parts == NULL) abort();
    size_t length = 0;
    for (int64_t i = 0; i < n; i++) {
        char text[64];
        int written = snprintf(text, sizeof text, "item-%lld:%lld", (long long)i, (long long)(i * 7 % 1000));
        parts[i] = malloc((size_t)written + 1);
        if (parts[i] == NULL) abort();
        memcpy(parts[i], text, (size_t)written + 1);
        length += (size_t)written;
    }
    length += (size_t)(n - 1);
    char *joined = malloc(length + 1);
    if (joined == NULL) abort();
    size_t at = 0;
    for (int64_t i = 0; i < n; i++) {
        if (i > 0) joined[at++] = ',';
        size_t size = strlen(parts[i]);
        memcpy(joined + at, parts[i], size);
        at += size;
        free(parts[i]);
    }
    joined[at] = '\0';
    free(parts);
    int64_t total = 0;
    char *start = joined;
    for (;;) {
        char *comma = strchr(start, ',');
        size_t size = comma != NULL ? (size_t)(comma - start) : strlen(start);
        char *piece = malloc(size + 1);
        if (piece == NULL) abort();
        memcpy(piece, start, size);
        piece[size] = '\0';
        char *colon = strchr(piece, ':');
        total += (int64_t)(colon - piece) + strtoll(colon + 1, NULL, 10);
        free(piece);
        if (comma == NULL) break;
        start = comma + 1;
    }
    printf("%lld %lld\n", (long long)at, (long long)total);
    free(joined);
    return 0;
}

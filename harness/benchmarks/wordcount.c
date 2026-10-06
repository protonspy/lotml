#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* An open-addressing table of words and their counts, doubled when half full. */
typedef struct entry {
    char *word;
    int64_t count;
} entry;

static entry *table;
static size_t capacity = 1024, used = 0;

static uint64_t hash(const char *s) {
    uint64_t h = 1469598103934665603ULL;
    for (; *s; s++) h = (h ^ (unsigned char)*s) * 1099511628211ULL;
    return h;
}

static void put(char *word, int64_t count);

static void grow(void) {
    entry *old = table;
    size_t old_capacity = capacity;
    capacity *= 2;
    table = calloc(capacity, sizeof(entry));
    if (table == NULL) abort();
    used = 0;
    for (size_t i = 0; i < old_capacity; i++) {
        if (old[i].word != NULL) put(old[i].word, old[i].count);
    }
    free(old);
}

static void put(char *word, int64_t count) {
    size_t i = hash(word) & (capacity - 1);
    while (table[i].word != NULL) i = (i + 1) & (capacity - 1);
    table[i].word = word;
    table[i].count = count;
    used++;
}

static void add(const char *word) {
    size_t i = hash(word) & (capacity - 1);
    while (table[i].word != NULL) {
        if (strcmp(table[i].word, word) == 0) {
            table[i].count++;
            return;
        }
        i = (i + 1) & (capacity - 1);
    }
    if (2 * (used + 1) > capacity) {
        grow();
        add(word);
        return;
    }
    size_t size = strlen(word);
    char *copy = malloc(size + 1);
    if (copy == NULL) abort();
    memcpy(copy, word, size + 1);
    put(copy, 1);
}

int main(void) {
    table = calloc(capacity, sizeof(entry));
    if (table == NULL) abort();
    for (int64_t i = 0; i < 2000000; i++) {
        char word[32];
        snprintf(word, sizeof word, "w%lld", (long long)(i * 7919 % 5003));
        add(word);
    }
    int64_t best = 0;
    for (size_t i = 0; i < capacity; i++) {
        if (table[i].word != NULL && table[i].count > best) best = table[i].count;
    }
    printf("%lld %lld\n", (long long)used, (long long)best);
    for (size_t i = 0; i < capacity; i++) free(table[i].word);
    free(table);
    return 0;
}

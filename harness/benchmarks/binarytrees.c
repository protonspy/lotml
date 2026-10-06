#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

typedef struct node {
    struct node *left, *right;
} node;

static node *make(int64_t depth) {
    node *n = malloc(sizeof(node));
    if (n == NULL) abort();
    if (depth == 0) {
        n->left = n->right = NULL;
    } else {
        n->left = make(depth - 1);
        n->right = make(depth - 1);
    }
    return n;
}

static int64_t check(const node *n) {
    if (n == NULL) return 0;
    return 1 + check(n->left) + check(n->right);
}

static void release(node *n) {
    if (n == NULL) return;
    release(n->left);
    release(n->right);
    free(n);
}

int main(void) {
    int64_t max_depth = 16;
    node *stretch = make(max_depth + 1);
    int64_t total = check(stretch);
    release(stretch);
    node *long_lived = make(max_depth);
    for (int64_t depth = 4; depth <= max_depth; depth += 2) {
        int64_t iterations = (int64_t)1 << (max_depth - depth + 4);
        int64_t nodes = 0;
        for (int64_t k = 0; k < iterations; k++) {
            node *t = make(depth);
            nodes += check(t);
            release(t);
        }
        total += nodes;
    }
    total += check(long_lived);
    release(long_lived);
    printf("%lld\n", (long long)total);
    return 0;
}

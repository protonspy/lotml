/* The runtime of C programs compiled from lotml (specs/c-backend, adr:0014).
 *
 * Included with lotml.c into each compiled program, which is one translation unit. Values on the
 * heap are cells that start with a count: positive while one task holds them, negative once
 * marked shared and updated atomically, zero for static cells never counted or freed. */

#ifndef LOTML_H
#define LOTML_H

#include <math.h>
#include <setjmp.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#if defined(__GNUC__) || defined(__clang__)
#define LT_GNU 1
#define LT_NORETURN __attribute__((noreturn))
#define LT_LIKELY(x) __builtin_expect(!!(x), 1)
#define LT_UNLIKELY(x) __builtin_expect(!!(x), 0)
#else
#define LT_GNU 0
#define LT_NORETURN __declspec(noreturn)
#define LT_LIKELY(x) (x)
#define LT_UNLIKELY(x) (x)
#endif

/* Where a panic happened: the `.lotml` file, the line and the function. */
typedef struct lt_at {
    const char *file;
    int line;
    const char *function;
} lt_at;

/* Stop the program: `panic: <kind>: <message>` and the place, exit status 101 (R2.2). */
LT_NORETURN void lt_panic(lt_at at, const char *kind, const char *message);
LT_NORETURN void lt_panicf(lt_at at, const char *kind, const char *format, ...);

/* The header of every heap cell. */
typedef struct lt_cell {
    int32_t count;
    uint32_t aux;
} lt_cell;

void *lt_alloc(size_t size);
void lt_free(void *cell);
int32_t lt_atomic_add(int32_t *count, int32_t delta);

static inline void lt_inc(void *p) {
    lt_cell *c = (lt_cell *)p;
    if (LT_LIKELY(c->count > 0)) {
        c->count++;
    } else if (c->count < 0) {
        lt_atomic_add(&c->count, -1);
    }
}

/* Take one count off `p`; true when it was the last, and the caller frees the cell. */
static inline bool lt_dec(void *p) {
    lt_cell *c = (lt_cell *)p;
    if (LT_LIKELY(c->count > 1)) {
        c->count--;
        return false;
    }
    if (c->count == 1) {
        return true;
    }
    if (c->count == 0) {
        return false;
    }
    return lt_atomic_add(&c->count, 1) == 0;
}

/* Whether the caller holds the only count: a value it may change in place or reuse. */
static inline bool lt_unique(const void *p) {
    return ((const lt_cell *)p)->count == 1;
}

/* Cells allocated and not yet freed: what a test build reports at exit (R3.6). */
extern int64_t lt_live_cells;

/* Output: standard output is written through one buffer, flushed at exit and before a panic. */
void lt_write(const char *bytes, size_t length);
void lt_flush(void);
void lt_init(void);
int lt_exit(int status);

#endif

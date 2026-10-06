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

#if LT_GNU
#define LT_UNUSED __attribute__((unused))
#else
#define LT_UNUSED
#endif

#if defined(_MSC_VER) && !defined(__clang__)
#include <intrin.h>
#endif

/* Where a panic happened: the `.lotml` file, the line and the function. */
typedef struct lt_at {
    const char *file;
    int line;
    const char *function;
} lt_at;

/* Where the code being run is: generated functions declare `lt_fn`, their lotml name, and the
 * `#line` directives make `__FILE__` and `__LINE__` the `.lotml` file and line. */
#define LT_HERE ((lt_at){__FILE__, __LINE__, lt_fn})

/* Stop the program: `panic: <kind>: <message>` and the place, exit status 101 (R2.2). */
LT_NORETURN void lt_panic(lt_at at, const char *kind, const char *message);
LT_NORETURN void lt_panicf(lt_at at, const char *kind, const char *format, ...);
LT_NORETURN void lt_overflow(lt_at at, const char *type);
LT_NORETURN void lt_zero_division(lt_at at, const char *message);
LT_NORETURN void lt_value_error(lt_at at, const char *message);
LT_NORETURN void lt_todo(lt_at at);
LT_NORETURN void lt_assert_failed(lt_at at, const char *expression);

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

/* Text built up before it is written: `print`, `str`, f-strings. */
typedef struct lt_buf {
    char *data;
    size_t len;
    size_t cap;
} lt_buf;

#define LT_BUF ((lt_buf){NULL, 0, 0})
void lt_buf_put(lt_buf *b, const char *bytes, size_t length);
void lt_buf_puts(lt_buf *b, const char *text);
void lt_buf_free(lt_buf *b);
void lt_buf_i64(lt_buf *b, int64_t value);
void lt_buf_u64(lt_buf *b, uint64_t value);
void lt_buf_f64(lt_buf *b, double value);
void lt_buf_bool(lt_buf *b, bool value);

/* Integer arithmetic, checked against the type's range (R2.1). `int` is i64; the narrower kinds
 * compute in i64 and are checked against their own range; u64 has its own functions. */

static inline int64_t lt_add_i64(int64_t a, int64_t b, lt_at at) {
#if LT_GNU
    int64_t r;
    if (__builtin_add_overflow(a, b, &r)) lt_overflow(at, "int");
    return r;
#else
    if ((b > 0 && a > INT64_MAX - b) || (b < 0 && a < INT64_MIN - b)) lt_overflow(at, "int");
    return a + b;
#endif
}

static inline int64_t lt_sub_i64(int64_t a, int64_t b, lt_at at) {
#if LT_GNU
    int64_t r;
    if (__builtin_sub_overflow(a, b, &r)) lt_overflow(at, "int");
    return r;
#else
    if ((b < 0 && a > INT64_MAX + b) || (b > 0 && a < INT64_MIN + b)) lt_overflow(at, "int");
    return a - b;
#endif
}

static inline int64_t lt_mul_i64(int64_t a, int64_t b, lt_at at) {
#if LT_GNU
    int64_t r;
    if (__builtin_mul_overflow(a, b, &r)) lt_overflow(at, "int");
    return r;
#elif defined(_M_X64)
    int64_t high;
    int64_t low = _mul128(a, b, &high);
    if (high != (low >> 63)) lt_overflow(at, "int");
    return low;
#else
    if (a != 0 && b != 0) {
        if ((a == -1 && b == INT64_MIN) || (b == -1 && a == INT64_MIN)) lt_overflow(at, "int");
        if (a > 0 ? (b > 0 ? a > INT64_MAX / b : b < INT64_MIN / a) : (b > 0 ? a < INT64_MIN / b : a < INT64_MAX / b))
            lt_overflow(at, "int");
    }
    return a * b;
#endif
}

static inline int64_t lt_neg_i64(int64_t a, lt_at at) {
    if (a == INT64_MIN) lt_overflow(at, "int");
    return -a;
}

static inline int64_t lt_abs_i64(int64_t a, lt_at at) {
    if (a == INT64_MIN) lt_overflow(at, "int");
    return a < 0 ? -a : a;
}

/* `//`: the quotient rounded toward negative infinity, as Python. */
static inline int64_t lt_floordiv_i64(int64_t a, int64_t b, lt_at at) {
    if (b == 0) lt_zero_division(at, "integer division or modulo by zero");
    if (b == -1) return lt_neg_i64(a, at);
    int64_t q = a / b;
    if ((a % b != 0) && ((a < 0) != (b < 0))) q--;
    return q;
}

/* `%`: the remainder with the divisor's sign, as Python. */
static inline int64_t lt_mod_i64(int64_t a, int64_t b, lt_at at) {
    if (b == 0) lt_zero_division(at, "integer division or modulo by zero");
    if (b == -1) return 0;
    int64_t r = a % b;
    if (r != 0 && ((r < 0) != (b < 0))) r += b;
    return r;
}

static inline double lt_truediv_i64(int64_t a, int64_t b, lt_at at) {
    if (b == 0) lt_zero_division(at, "division by zero");
    return (double)a / (double)b;
}

int64_t lt_pow_i64(int64_t base, int64_t exponent, lt_at at);
int64_t lt_shl_i64(int64_t value, int64_t amount, lt_at at);
int64_t lt_shr_i64(int64_t value, int64_t amount, lt_at at);

static inline uint64_t lt_add_u64(uint64_t a, uint64_t b, lt_at at) {
    uint64_t r = a + b;
    if (r < a) lt_overflow(at, "u64");
    return r;
}

static inline uint64_t lt_sub_u64(uint64_t a, uint64_t b, lt_at at) {
    if (a < b) lt_overflow(at, "u64");
    return a - b;
}

static inline uint64_t lt_mul_u64(uint64_t a, uint64_t b, lt_at at) {
#if LT_GNU
    uint64_t r;
    if (__builtin_mul_overflow(a, b, &r)) lt_overflow(at, "u64");
    return r;
#elif defined(_M_X64)
    uint64_t high;
    uint64_t low = _umul128(a, b, &high);
    if (high != 0) lt_overflow(at, "u64");
    return low;
#else
    if (a != 0 && b > UINT64_MAX / a) lt_overflow(at, "u64");
    return a * b;
#endif
}

static inline uint64_t lt_floordiv_u64(uint64_t a, uint64_t b, lt_at at) {
    if (b == 0) lt_zero_division(at, "integer division or modulo by zero");
    return a / b;
}

static inline uint64_t lt_mod_u64(uint64_t a, uint64_t b, lt_at at) {
    if (b == 0) lt_zero_division(at, "integer division or modulo by zero");
    return a % b;
}

static inline double lt_truediv_u64(uint64_t a, uint64_t b, lt_at at) {
    if (b == 0) lt_zero_division(at, "division by zero");
    return (double)a / (double)b;
}

/* `-x` and `~x` of a u64 are negative, outside u64, unless `-0`. */
static inline uint64_t lt_neg_u64(uint64_t a, lt_at at) {
    if (a != 0) lt_overflow(at, "u64");
    return 0;
}

uint64_t lt_pow_u64(uint64_t base, uint64_t exponent, lt_at at);
uint64_t lt_shl_u64(uint64_t value, uint64_t amount, lt_at at);
uint64_t lt_shr_u64(uint64_t value, uint64_t amount, lt_at at);

/* A narrower integer's result, computed in i64, checked against its own range. */
static inline int64_t lt_fit(int64_t value, int64_t low, int64_t high, const char *type, lt_at at) {
    if (value < low || value > high) lt_overflow(at, type);
    return value;
}

/* Conversions: `int(x)`, `float(n)`, `i32(n)` and the rest. */
int64_t lt_f64_to_i64(double value, lt_at at);
uint64_t lt_f64_to_u64(double value, lt_at at);
static inline uint64_t lt_i64_to_u64(int64_t value, lt_at at) {
    if (value < 0) lt_overflow(at, "u64");
    return (uint64_t)value;
}
static inline int64_t lt_u64_to_i64(uint64_t value, lt_at at) {
    if (value > (uint64_t)INT64_MAX) lt_overflow(at, "int");
    return (int64_t)value;
}

/* Float arithmetic as Python's: division by zero stops the program, `//` and `%` floor. */
static inline double lt_truediv_f64(double a, double b, lt_at at) {
    if (b == 0.0) lt_zero_division(at, "float division by zero");
    return a / b;
}
double lt_floordiv_f64(double a, double b, lt_at at);
double lt_mod_f64(double a, double b, lt_at at);
double lt_pow_f64(double a, double b, lt_at at);

static inline double lt_min_f64(double a, double b) {
    return b < a ? b : a;
}
static inline double lt_max_f64(double a, double b) {
    return b > a ? b : a;
}

/* `range(start, stop, step)` as a `for` walks it: no value past `stop`, and no overflow. */
typedef struct lt_range {
    int64_t next;
    int64_t stop;
    int64_t step;
    bool done;
} lt_range;

lt_range lt_range_new(int64_t start, int64_t stop, int64_t step, lt_at at);

static inline bool lt_range_step(lt_range *r, int64_t *value) {
    if (r->done || (r->step > 0 ? r->next >= r->stop : r->next <= r->stop)) {
        return false;
    }
    *value = r->next;
#if LT_GNU
    if (__builtin_add_overflow(r->next, r->step, &r->next)) r->done = true;
#else
    if ((r->step > 0 && r->next > INT64_MAX - r->step) || (r->step < 0 && r->next < INT64_MIN - r->step)) {
        r->done = true;
    } else {
        r->next += r->step;
    }
#endif
    return true;
}

/* Strings ---------------------------------------------------------------------------------- */

/* `str`: immutable UTF-8 text in a cell. `length` counts code points, so a string is ASCII when
 * it equals `size`; `hash` is 0 until computed. */
typedef struct lt_str {
    lt_cell cell;
    int64_t size;
    int64_t length;
    uint64_t hash;
    char bytes[];
} lt_str;

/* A string literal: a static cell, never counted or freed. */
#define LT_STR_LITERAL(name, size, length, text) \
    static struct {                               \
        lt_cell cell;                             \
        int64_t s;                                \
        int64_t l;                                \
        uint64_t h;                               \
        char bytes[(size) + 1];                   \
    } name LT_UNUSED = {{0, 0}, (size), (length), 0, text}

lt_str *lt_str_new(const char *bytes, int64_t size);
lt_str *lt_str_from_buf(lt_buf *b);
void lt_str_drop(lt_str *s);
void lt_buf_str(lt_buf *b, const lt_str *s);
void lt_buf_str_repr(lt_buf *b, const lt_str *s);
lt_str *lt_str_of_i64(int64_t value);
lt_str *lt_str_of_u64(uint64_t value);
lt_str *lt_str_of_f64(double value);
lt_str *lt_str_of_bool(bool value);
lt_str *lt_str_none(void);

lt_str *lt_str_concat(const lt_str *a, const lt_str *b);
lt_str *lt_str_repeat(const lt_str *s, int64_t times, lt_at at);
int lt_str_compare(const lt_str *a, const lt_str *b);
bool lt_str_eq(const lt_str *a, const lt_str *b);
bool lt_str_contains(const lt_str *haystack, const lt_str *needle);
lt_str *lt_str_index(const lt_str *s, int64_t index, lt_at at);
lt_str *lt_str_slice(const lt_str *s, bool has_lo, int64_t lo, bool has_hi, int64_t hi, bool has_step, int64_t step, lt_at at);
int64_t lt_str_ord(const lt_str *s, lt_at at);
lt_str *lt_str_chr(int64_t code, lt_at at);

lt_str *lt_str_lower(const lt_str *s);
lt_str *lt_str_upper(const lt_str *s);
lt_str *lt_str_swapcase(const lt_str *s);
lt_str *lt_str_title(const lt_str *s);
lt_str *lt_str_capitalize(const lt_str *s);
lt_str *lt_str_strip(const lt_str *s, const lt_str *chars, bool left, bool right);
bool lt_str_startswith(const lt_str *s, const lt_str *prefix);
bool lt_str_endswith(const lt_str *s, const lt_str *suffix);
lt_str *lt_str_replace(const lt_str *s, const lt_str *old, const lt_str *with, int64_t count);
int64_t lt_str_count(const lt_str *s, const lt_str *sub);
bool lt_str_isalpha(const lt_str *s);
bool lt_str_isdigit(const lt_str *s);
bool lt_str_isspace(const lt_str *s);
bool lt_str_isalnum(const lt_str *s);
bool lt_str_isupper(const lt_str *s);
bool lt_str_islower(const lt_str *s);
lt_str *lt_str_zfill(const lt_str *s, int64_t width);
lt_str *lt_str_pad(const lt_str *s, int64_t width, const lt_str *fill, char align, lt_at at);

/* The format mini-language: `format(value, spec)` as Python writes it, into `b`. */
void lt_format_i64(lt_buf *b, int64_t value, const char *spec, size_t size, lt_at at);
void lt_format_u64(lt_buf *b, uint64_t value, const char *spec, size_t size, lt_at at);
void lt_format_f64(lt_buf *b, double value, const char *spec, size_t size, lt_at at);
void lt_format_bool(lt_buf *b, bool value, const char *spec, size_t size, lt_at at);
void lt_format_str(lt_buf *b, const lt_str *value, const char *spec, size_t size, lt_at at);
void lt_format_none(lt_buf *b, const char *spec, size_t size, lt_at at);

#endif

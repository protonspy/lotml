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
#define LT_THREAD __thread
#else
#define LT_UNUSED
#define LT_THREAD __declspec(thread)
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
void *lt_reuse_or_alloc(void *token, size_t token_size, size_t size);
void lt_free(void *cell);
int32_t lt_atomic_add(int32_t *count, int32_t delta);

/* The count of `c`, read atomically: a shared cell's changes under other threads, and only its
 * sign, which never turns back, decides how this thread touches it. */
static inline int32_t lt_count_of(const lt_cell *c) {
#if LT_GNU
    return __atomic_load_n(&c->count, __ATOMIC_RELAXED);
#else
    return *(volatile const int32_t *)&c->count;
#endif
}

static inline void lt_inc(void *p) {
    lt_cell *c = (lt_cell *)p;
    int32_t n = lt_count_of(c);
    if (LT_LIKELY(n > 0)) {
        c->count = n + 1;
    } else if (n < 0) {
        lt_atomic_add(&c->count, -1);
    }
}

/* Take one count off `p`; true when it was the last, and the caller frees the cell. */
static inline bool lt_dec(void *p) {
    lt_cell *c = (lt_cell *)p;
    int32_t n = lt_count_of(c);
    if (LT_LIKELY(n > 1)) {
        c->count = n - 1;
        return false;
    }
    if (n == 1) {
        return true;
    }
    if (n == 0) {
        return false;
    }
    return lt_atomic_add(&c->count, 1) == 0;
}

/* Whether the caller holds the only count: a value it may change in place or reuse. */
static inline bool lt_unique(const void *p) {
    return lt_count_of((const lt_cell *)p) == 1;
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

/* The prelude's arithmetic, as Python computes it ----------------------------------------- */

int64_t lt_round_i64(double x, lt_at at);
double lt_round_f64(double x, int64_t digits);
int64_t lt_pow_mod(int64_t base, int64_t exponent, int64_t modulus, lt_at at);
int64_t lt_isqrt(int64_t n, lt_at at);
int64_t lt_gcd(int64_t a, int64_t b, lt_at at);
static inline int64_t lt_wrapping_add(int64_t a, int64_t b) {
    return (int64_t)((uint64_t)a + (uint64_t)b);
}
static inline int64_t lt_wrapping_sub(int64_t a, int64_t b) {
    return (int64_t)((uint64_t)a - (uint64_t)b);
}
static inline int64_t lt_wrapping_mul(int64_t a, int64_t b) {
    return (int64_t)((uint64_t)a * (uint64_t)b);
}

/* `math`: a result that is not a number, or infinite, from finite arguments stops the program as
 * CPython's module does. */
double lt_math_1(double (*f)(double), double x, bool can_overflow, lt_at at);
double lt_math_2(double (*f)(double, double), double x, double y, lt_at at);
double lt_math_log(double (*f)(double), double x, lt_at at);
double lt_math_pow(double x, double y, lt_at at);
int64_t lt_factorial(int64_t n, lt_at at);
int64_t lt_comb(int64_t n, int64_t k, lt_at at);
int64_t lt_perm(int64_t n, int64_t k, lt_at at);
static inline double lt_math_sqrt(double x, lt_at at) {
    return lt_math_1(sqrt, x, false, at);
}
static inline double lt_math_exp(double x, lt_at at) {
    return lt_math_1(exp, x, true, at);
}
static inline double lt_math_sin(double x, lt_at at) {
    return lt_math_1(sin, x, false, at);
}
static inline double lt_math_cos(double x, lt_at at) {
    return lt_math_1(cos, x, false, at);
}
static inline double lt_math_tan(double x, lt_at at) {
    return lt_math_1(tan, x, false, at);
}
static inline double lt_math_atan(double x, lt_at at) {
    return lt_math_1(atan, x, false, at);
}
static inline double lt_math_fabs(double x, lt_at at) {
    (void)at;
    return fabs(x);
}
static inline double lt_math_ln(double x, lt_at at) {
    return lt_math_log(log, x, at);
}
static inline double lt_math_log2(double x, lt_at at) {
    return lt_math_log(log2, x, at);
}
static inline double lt_math_log10(double x, lt_at at) {
    return lt_math_log(log10, x, at);
}
static inline double lt_math_atan2(double y, double x, lt_at at) {
    return lt_math_2(atan2, y, x, at);
}
static inline double lt_math_hypot(double x, double y, lt_at at) {
    return lt_math_2(hypot, x, y, at);
}
static inline int64_t lt_math_floor(double x, lt_at at) {
    return lt_f64_to_i64(floor(x), at);
}
static inline int64_t lt_math_ceil(double x, lt_at at) {
    return lt_f64_to_i64(ceil(x), at);
}
static inline int64_t lt_math_trunc(double x, lt_at at) {
    return lt_f64_to_i64(trunc(x), at);
}

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

/* Types ------------------------------------------------------------------------------------ */

/* What the runtime needs to know of a type to hold its values in a collection: their size, how
 * to count and drop what a value holds, compare, order, hash and write it, and mark it shared.
 * `inc` and `dec` are NULL for a type that holds nothing counted; `hash` for one that cannot be
 * hashed. Every function takes a pointer to a value. */
typedef struct lt_type {
    size_t size;
    void (*inc)(void *value);
    void (*dec)(void *value);
    bool (*eq)(const void *a, const void *b);
    int (*cmp)(const void *a, const void *b, lt_at at);
    int64_t (*hash)(const void *value);
    void (*repr)(lt_buf *b, const void *value);
    void (*str)(lt_buf *b, const void *value);
    void (*share)(void *value);
    /* the value as a test report shows it: `show` of the Python target's runtime */
    void (*show)(lt_buf *b, const void *value);
} lt_type;

/* Test blocks: each run by `lt_run_test`, a failed `assert`, an error passed on by `?` or a
 * panic caught and kept; `lt_test_report` writes what `lotml test` reads (R1.4). */
typedef void (*lt_test_fn)(void);
void lt_run_test(const char *name, lt_test_fn test);
void lt_test_report(void);
LT_NORETURN void lt_assert_fail(lt_at at, const char *expression, const char *op, const lt_type *lt, const void *l,
                                const lt_type *rt, const void *r, const lt_type *mt, const void *m);
LT_NORETURN void lt_test_error(const lt_type *type, const void *error, lt_at at);
LT_NORETURN void lt_assert_compared(const lt_str *expression, const lt_str *op, const lt_type *lt, const void *l,
                                    const lt_type *rt, const void *r, const lt_type *mt, const void *m, lt_at at);
void lt_buf_json(lt_buf *b, const char *text, size_t length);

extern const lt_type lt_type_i8, lt_type_i16, lt_type_i32, lt_type_i64;
extern const lt_type lt_type_u8, lt_type_u16, lt_type_u32, lt_type_u64;
extern const lt_type lt_type_f64, lt_type_bool, lt_type_none, lt_type_str, lt_type_list;

/* A value of any type as text: `str(value)`, and the f-string field `{value:spec}`. */
void lt_buf_value(lt_buf *b, const lt_type *type, const void *value);
lt_str *lt_str_of_value(const lt_type *type, const void *value);
void lt_format_value(lt_buf *b, const lt_type *type, const void *value, const char *spec, size_t size, lt_at at);
LT_NORETURN void lt_unorderable(lt_at at, const char *type);

int64_t lt_hash_i64(int64_t value);
int64_t lt_hash_u64(uint64_t value);
int64_t lt_hash_f64(double value);
int64_t lt_hash_part(const lt_type *type, const void *value);
int64_t lt_hash_tuple(const int64_t *lanes, int64_t n);

/* Lists ------------------------------------------------------------------------------------ */

/* `[T]`: `len` elements of `type`, stored inline in `data`, which grows apart from the cell. */
typedef struct lt_list {
    lt_cell cell;
    int64_t len;
    int64_t cap;
    const lt_type *type;
    char *data;
} lt_list;

LT_NORETURN void lt_index_error(lt_at at, const char *message);

/* `index` of a sequence of `len`, negative from the end, checked. */
static inline int64_t lt_index(int64_t len, int64_t index, lt_at at) {
    if (index < 0) index += len;
    if (LT_UNLIKELY(index < 0 || index >= len)) lt_index_error(at, "list index out of range");
    return index;
}

lt_list *lt_list_new(const lt_type *type, int64_t cap);
void lt_list_drop(lt_list *l);
void lt_list_inc(void *value);
void lt_list_dec(void *value);
void lt_list_unique(lt_list **slot);
void *lt_list_slot(lt_list **slot, int64_t index, lt_at at);
void lt_list_push(lt_list **slot, const void *value);
void lt_list_extend(lt_list **slot, const lt_list *other);
void lt_list_insert(lt_list **slot, int64_t index, const void *value);
void lt_list_remove(lt_list **slot, const void *value, lt_at at);
void lt_list_clear(lt_list **slot);
void lt_list_reverse(lt_list **slot);
void lt_list_sort(lt_list **slot, bool reverse, lt_at at);
lt_list *lt_list_copy(const lt_list *l);
lt_list *lt_list_sorted(const lt_list *l, bool reverse, lt_at at);
lt_list *lt_list_reversed(const lt_list *l);
lt_list *lt_list_slice(const lt_list *l, bool has_lo, int64_t lo, bool has_hi, int64_t hi, bool has_step, int64_t step, lt_at at);
lt_list *lt_list_concat(const lt_list *a, const lt_list *b);
lt_list *lt_list_repeat(const lt_list *l, int64_t times, lt_at at);
bool lt_list_contains(const lt_list *l, const void *value);
int64_t lt_list_count(const lt_list *l, const void *value);
const void *lt_list_extreme(const lt_list *l, bool max, lt_at at);
bool lt_list_any(const lt_list *l);
bool lt_list_all(const lt_list *l);
int64_t lt_sum_i64(const lt_list *l, lt_at at);
uint64_t lt_sum_u64(const lt_list *l, lt_at at);
double lt_sum_f64(const lt_list *l);
lt_list *lt_range_list(int64_t start, int64_t stop, int64_t step, lt_at at);
lt_list *lt_str_chars(const lt_str *s);
lt_list *lt_str_split(const lt_str *s, const lt_str *sep, int64_t maxsplit, lt_at at);
lt_list *lt_str_splitlines(const lt_str *s);
lt_str *lt_str_join(const lt_str *sep, const lt_list *parts);
lt_str *lt_str_partition_part(const lt_str *s, const lt_str *sep, int which, lt_at at);

/* Functions giving an optional value: true and the value written to `out`, or false and `out`
 * zeroed — a NULL string or list — so that the local it names is always set. */
bool lt_str_to_int(const lt_str *s, int64_t *out, lt_at at);
bool lt_str_to_float(const lt_str *s, double *out);
bool lt_str_find(const lt_str *s, const lt_str *sub, bool last, int64_t *out);
bool lt_str_split_once(const lt_str *s, const lt_str *sep, lt_str **head, lt_str **tail, lt_at at);
bool lt_list_index(const lt_list *l, const void *value, int64_t *out);
bool lt_list_pop(lt_list **slot, bool has_index, int64_t index, void *out, lt_at at);
bool lt_list_last(const lt_list *l, void *out);

/* `Heap[T]`: a list kept in the order Python's `heapq` keeps it, so ties pop as they do there. */
extern const lt_type lt_type_heap;
void lt_heapify(lt_list **slot, lt_at at);
void lt_heap_push(lt_list **slot, const void *value, lt_at at);
bool lt_heap_pop(lt_list **slot, void *out, lt_at at);
bool lt_heap_peek(const lt_list *l, void *out);
int64_t lt_hash_value(const lt_type *type, const void *value, lt_at at);
void lt_slice_indices(int64_t length, bool has_lo, int64_t lo, bool has_hi, int64_t hi, bool has_step, int64_t step,
                      int64_t *start, int64_t *count, int64_t *by, lt_at at);

/* Functions as values --------------------------------------------------------------------- */

/* A closure: the C function a call goes to, what drops the values it captured and what marks
 * them shared, then those values. A named function used as a value is a static closure with no
 * captures. */
typedef struct lt_closure {
    lt_cell cell;
    void *fn;
    void (*drop)(struct lt_closure *self);
    void (*share)(struct lt_closure *self);
} lt_closure;

extern const lt_type lt_type_closure;
void lt_closure_drop(lt_closure *c);

/* `parallel(tasks)` (R4.1): each task run by `run`, which calls it and stores its result of
 * `result` at `out`, on a thread of its own, at most 256 at once; the results in order. What the
 * tasks capture is marked shared first (R3.3). A task that panics stops the program once the
 * others have finished, with the first such task's panic. */
typedef void (*lt_task_fn)(lt_closure *task, void *out);
lt_list *lt_parallel(const lt_list *tasks, const lt_type *result, lt_task_fn run, lt_at at);

/* `xs.sort(key=f)`: the elements ordered by `keys`, one per element, stably. */
void lt_list_sort_by_keys(lt_list **slot, const lt_list *keys, bool reverse, lt_at at);

/* `int(s)` and `float(s)` of a string: the number, or the program stops with ValueError. */
int64_t lt_str_int(const lt_str *s, lt_at at);
double lt_str_float(const lt_str *s, lt_at at);

/* Dicts and sets --------------------------------------------------------------------------- */

/* `{K: V}`: entries in insertion order — hash, liveness, key, value — behind an index table. */
typedef struct lt_dict {
    lt_cell cell;
    const lt_type *key;
    const lt_type *value;
    int64_t len;
    int64_t used;
    int64_t cap;
    char *entries;
    int64_t *index;
    int64_t mask;
} lt_dict;

/* `{T}`: CPython's open-addressing set table, slot for slot. */
typedef struct lt_set {
    lt_cell cell;
    const lt_type *type;
    int64_t fill;
    int64_t used;
    int64_t mask;
    int64_t finger;
    char *table;
} lt_set;

extern const lt_type lt_type_dict, lt_type_set;
int64_t lt_hash_str(lt_str *s);

lt_dict *lt_dict_new(const lt_type *key, const lt_type *value);
void lt_dict_drop(lt_dict *d);
lt_dict *lt_dict_copy(const lt_dict *d);
void lt_dict_set(lt_dict **slot, const void *key, const void *value);
const void *lt_dict_get(const lt_dict *d, const void *key, lt_at at);
const void *lt_dict_get_or(const lt_dict *d, const void *key, const void *otherwise);
bool lt_dict_get_optional(const lt_dict *d, const void *key, void *out);
void *lt_dict_slot(lt_dict **slot, const void *key, lt_at at);
void *lt_dict_setdefault(lt_dict **slot, const void *key, const void *otherwise);
bool lt_dict_contains(const lt_dict *d, const void *key);
bool lt_dict_pop(lt_dict **slot, const void *key, void *out);
void lt_dict_clear(lt_dict **slot);
lt_list *lt_dict_keys(const lt_dict *d);
lt_list *lt_dict_values(const lt_dict *d);
lt_list *lt_dict_items(const lt_dict *d, const lt_type *pair, size_t value_at);
lt_dict *lt_dict_from_pairs(const lt_type *key, const lt_type *value, const lt_list *pairs, size_t value_at);

lt_set *lt_set_new(const lt_type *type);
void lt_set_drop(lt_set *s);
lt_set *lt_set_copy(const lt_set *s);
void lt_set_add(lt_set **slot, const void *key);
void lt_set_discard(lt_set **slot, const void *key);
void lt_set_remove(lt_set **slot, const void *key, lt_at at);
bool lt_set_pop(lt_set **slot, void *out);
bool lt_set_contains(const lt_set *s, const void *key);
lt_list *lt_set_list(const lt_set *s);
lt_set *lt_set_from_list(const lt_type *type, const lt_list *items);
lt_set *lt_set_union(const lt_set *a, const lt_set *b);
lt_set *lt_set_intersection(const lt_set *a, const lt_set *b);
lt_set *lt_set_difference(const lt_set *a, const lt_set *b);
bool lt_set_issubset(const lt_set *a, const lt_set *b);

/* The format mini-language: `format(value, spec)` as Python writes it, into `b`. */
void lt_format_i64(lt_buf *b, int64_t value, const char *spec, size_t size, lt_at at);
void lt_format_u64(lt_buf *b, uint64_t value, const char *spec, size_t size, lt_at at);
void lt_format_f64(lt_buf *b, double value, const char *spec, size_t size, lt_at at);
void lt_format_bool(lt_buf *b, bool value, const char *spec, size_t size, lt_at at);
void lt_format_str(lt_buf *b, const lt_str *value, const char *spec, size_t size, lt_at at);
void lt_format_none(lt_buf *b, const char *spec, size_t size, lt_at at);

#endif

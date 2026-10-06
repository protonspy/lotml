/* The runtime's functions; see lotml.h. */

#include "lotml.h"

#include <stdarg.h>

#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif

/* Cells and output ------------------------------------------------------------------------- */

int64_t lt_live_cells = 0;

#ifdef LT_COUNT_CELLS
static void lt_count_cells(int64_t delta) {
#if LT_GNU
    __atomic_add_fetch(&lt_live_cells, delta, __ATOMIC_RELAXED);
#else
    _InterlockedExchangeAdd64((volatile long long *)&lt_live_cells, delta);
#endif
}
#endif

static char lt_out[1 << 16];
static size_t lt_out_used = 0;

void lt_flush(void) {
    if (lt_out_used > 0) {
        fwrite(lt_out, 1, lt_out_used, stdout);
        lt_out_used = 0;
    }
    fflush(stdout);
}

void lt_write(const char *bytes, size_t length) {
    if (length >= sizeof lt_out) {
        lt_flush();
        fwrite(bytes, 1, length, stdout);
        return;
    }
    if (lt_out_used + length > sizeof lt_out) {
        lt_flush();
    }
    memcpy(lt_out + lt_out_used, bytes, length);
    lt_out_used += length;
}

void lt_init(void) {
#ifdef _WIN32
    /* Bytes out as they are: no "\n" turned into "\r\n", as Python writes on every platform. */
    _setmode(_fileno(stdout), _O_BINARY);
    _setmode(_fileno(stderr), _O_BINARY);
#endif
}

int lt_exit(int status) {
    lt_flush();
#ifdef LT_COUNT_CELLS
    fprintf(stderr, "lotml: %lld cells live at exit\n", (long long)lt_live_cells);
#endif
    return status;
}

void *lt_alloc(size_t size) {
    void *cell = malloc(size);
    if (cell == NULL) {
        lt_panic((lt_at){NULL, 0, NULL}, "MemoryError", "out of memory");
    }
    ((lt_cell *)cell)->count = 1;
    ((lt_cell *)cell)->aux = 0;
#ifdef LT_COUNT_CELLS
    lt_count_cells(1);
#endif
    return cell;
}

void lt_free(void *cell) {
#ifdef LT_COUNT_CELLS
    lt_count_cells(-1);
#endif
    free(cell);
}

int32_t lt_atomic_add(int32_t *count, int32_t delta) {
#if LT_GNU
    return __atomic_add_fetch(count, delta, __ATOMIC_ACQ_REL);
#else
    return _InterlockedExchangeAdd((volatile long *)count, delta) + delta;
#endif
}

/* Panics ----------------------------------------------------------------------------------- */

void lt_panic(lt_at at, const char *kind, const char *message) {
    lt_flush();
    if (message != NULL && message[0] != '\0') {
        fprintf(stderr, "panic: %s: %s\n", kind, message);
    } else {
        fprintf(stderr, "panic: %s\n", kind);
    }
    if (at.file != NULL) {
        fprintf(stderr, "  File \"%s\", line %d, in %s\n", at.file, at.line, at.function);
    }
    fflush(stderr);
    exit(101);
}

void lt_panicf(lt_at at, const char *kind, const char *format, ...) {
    char message[512];
    va_list args;
    va_start(args, format);
    vsnprintf(message, sizeof message, format, args);
    va_end(args);
    lt_panic(at, kind, message);
}

void lt_overflow(lt_at at, const char *type) {
    lt_panicf(at, "Overflow", "the result does not fit in %s", type);
}

void lt_zero_division(lt_at at, const char *message) {
    lt_panic(at, "ZeroDivisionError", message);
}

void lt_value_error(lt_at at, const char *message) {
    lt_panic(at, "ValueError", message);
}

void lt_todo(lt_at at) {
    lt_panic(at, "Todo", "not written yet");
}

void lt_assert_failed(lt_at at, const char *expression) {
    lt_panic(at, "TestFailure", expression);
}

/* Text ------------------------------------------------------------------------------------- */

void lt_buf_put(lt_buf *b, const char *bytes, size_t length) {
    if (b->len + length > b->cap) {
        size_t cap = b->cap < 64 ? 64 : b->cap * 2;
        while (cap < b->len + length) cap *= 2;
        char *data = realloc(b->data, cap);
        if (data == NULL) lt_panic((lt_at){NULL, 0, NULL}, "MemoryError", "out of memory");
        b->data = data;
        b->cap = cap;
    }
    memcpy(b->data + b->len, bytes, length);
    b->len += length;
}

void lt_buf_puts(lt_buf *b, const char *text) {
    lt_buf_put(b, text, strlen(text));
}

void lt_buf_free(lt_buf *b) {
    free(b->data);
    *b = LT_BUF;
}

void lt_buf_i64(lt_buf *b, int64_t value) {
    char digits[24];
    int n = snprintf(digits, sizeof digits, "%lld", (long long)value);
    lt_buf_put(b, digits, (size_t)n);
}

void lt_buf_u64(lt_buf *b, uint64_t value) {
    char digits[24];
    int n = snprintf(digits, sizeof digits, "%llu", (unsigned long long)value);
    lt_buf_put(b, digits, (size_t)n);
}

void lt_buf_bool(lt_buf *b, bool value) {
    lt_buf_puts(b, value ? "True" : "False");
}

/* The shortest decimal digits that read back as `value`, and the exponent of the first digit:
 * `value` is 0.d1d2… × 10^(exponent + 1). `value` is finite and not zero. */
static int lt_shortest(double value, char digits[20], int *exponent) {
    char text[40];
    for (int precision = 1; precision <= 17; precision++) {
        snprintf(text, sizeof text, "%.*e", precision - 1, value);
        if (strtod(text, NULL) == value || precision == 17) {
            break;
        }
    }
    const char *p = text;
    if (*p == '-') p++;
    int n = 0;
    for (; *p != 'e'; p++) {
        if (*p != '.') digits[n++] = *p;
    }
    while (n > 1 && digits[n - 1] == '0') n--;
    digits[n] = '\0';
    *exponent = atoi(p + 1);
    return n;
}

/* `repr(value)`, as CPython writes a float: the shortest digits that read back, positional
 * between 1e-4 and 1e16, else in exponent form with at least two exponent digits. */
void lt_buf_f64(lt_buf *b, double value) {
    if (isnan(value)) {
        lt_buf_puts(b, "nan");
        return;
    }
    if (isinf(value)) {
        lt_buf_puts(b, value > 0 ? "inf" : "-inf");
        return;
    }
    if (value == 0.0) {
        lt_buf_puts(b, signbit(value) ? "-0.0" : "0.0");
        return;
    }
    char digits[20];
    int exponent;
    int n = lt_shortest(value, digits, &exponent);
    if (value < 0) lt_buf_put(b, "-", 1);
    int point = exponent + 1;
    if (point > -4 && point <= 16) {
        if (point <= 0) {
            lt_buf_put(b, "0.", 2);
            for (int i = 0; i < -point; i++) lt_buf_put(b, "0", 1);
            lt_buf_put(b, digits, (size_t)n);
        } else if (point >= n) {
            lt_buf_put(b, digits, (size_t)n);
            for (int i = n; i < point; i++) lt_buf_put(b, "0", 1);
            lt_buf_put(b, ".0", 2);
        } else {
            lt_buf_put(b, digits, (size_t)point);
            lt_buf_put(b, ".", 1);
            lt_buf_put(b, digits + point, (size_t)(n - point));
        }
        return;
    }
    lt_buf_put(b, digits, 1);
    if (n > 1) {
        lt_buf_put(b, ".", 1);
        lt_buf_put(b, digits + 1, (size_t)(n - 1));
    }
    char tail[8];
    int t = snprintf(tail, sizeof tail, "e%c%02d", exponent < 0 ? '-' : '+', exponent < 0 ? -exponent : exponent);
    lt_buf_put(b, tail, (size_t)t);
}

/* Integers --------------------------------------------------------------------------------- */

int64_t lt_pow_i64(int64_t base, int64_t exponent, lt_at at) {
    if (exponent < 0) lt_value_error(at, "a negative exponent of an int");
    int64_t result = 1;
    while (exponent > 0) {
        if (exponent & 1) result = lt_mul_i64(result, base, at);
        exponent >>= 1;
        if (exponent > 0) base = lt_mul_i64(base, base, at);
    }
    return result;
}

uint64_t lt_pow_u64(uint64_t base, uint64_t exponent, lt_at at) {
    uint64_t result = 1;
    while (exponent > 0) {
        if (exponent & 1) result = lt_mul_u64(result, base, at);
        exponent >>= 1;
        if (exponent > 0) base = lt_mul_u64(base, base, at);
    }
    return result;
}

int64_t lt_shl_i64(int64_t value, int64_t amount, lt_at at) {
    if (amount < 0) lt_value_error(at, "negative shift count");
    if (value == 0) return 0;
    if (amount >= 64) lt_overflow(at, "int");
    int64_t shifted = (int64_t)((uint64_t)value << amount);
    if ((shifted >> amount) != value) lt_overflow(at, "int");
    return shifted;
}

int64_t lt_shr_i64(int64_t value, int64_t amount, lt_at at) {
    if (amount < 0) lt_value_error(at, "negative shift count");
    if (amount >= 64) return value < 0 ? -1 : 0;
    return value >> amount;
}

uint64_t lt_shl_u64(uint64_t value, uint64_t amount, lt_at at) {
    if (value == 0) return 0;
    if (amount >= 64) lt_overflow(at, "u64");
    uint64_t shifted = value << amount;
    if ((shifted >> amount) != value) lt_overflow(at, "u64");
    return shifted;
}

uint64_t lt_shr_u64(uint64_t value, uint64_t amount, lt_at at) {
    (void)at;
    return amount >= 64 ? 0 : value >> amount;
}

int64_t lt_f64_to_i64(double value, lt_at at) {
    if (isnan(value)) lt_value_error(at, "cannot convert float NaN to integer");
    if (isinf(value)) lt_panic(at, "OverflowError", "cannot convert float infinity to integer");
    double whole = trunc(value);
    if (whole >= 9223372036854775808.0 || whole < -9223372036854775808.0) lt_overflow(at, "int");
    return (int64_t)whole;
}

uint64_t lt_f64_to_u64(double value, lt_at at) {
    if (isnan(value)) lt_value_error(at, "cannot convert float NaN to integer");
    if (isinf(value)) lt_panic(at, "OverflowError", "cannot convert float infinity to integer");
    double whole = trunc(value);
    if (whole >= 18446744073709551616.0 || whole < 0.0) lt_overflow(at, "u64");
    return (uint64_t)whole;
}

lt_range lt_range_new(int64_t start, int64_t stop, int64_t step, lt_at at) {
    if (step == 0) lt_value_error(at, "range() arg 3 must not be zero");
    return (lt_range){start, stop, step, false};
}

/* Floats, as CPython's float_divmod and float_pow ----------------------------------------- */

static void lt_divmod_f64(double a, double b, double *floordiv, double *mod) {
    double m = fmod(a, b);
    double div = (a - m) / b;
    if (m != 0.0) {
        if ((b < 0) != (m < 0)) {
            m += b;
            div -= 1.0;
        }
    } else {
        m = copysign(0.0, b);
    }
    double whole;
    if (div != 0.0) {
        whole = floor(div);
        if (div - whole > 0.5) whole += 1.0;
    } else {
        whole = copysign(0.0, a / b);
    }
    *floordiv = whole;
    *mod = m;
}

double lt_floordiv_f64(double a, double b, lt_at at) {
    if (b == 0.0) lt_zero_division(at, "float floor division by zero");
    double q, r;
    lt_divmod_f64(a, b, &q, &r);
    return q;
}

double lt_mod_f64(double a, double b, lt_at at) {
    if (b == 0.0) lt_zero_division(at, "float modulo by zero");
    double q, r;
    lt_divmod_f64(a, b, &q, &r);
    return r;
}

double lt_pow_f64(double a, double b, lt_at at) {
    if (b == 0.0) return 1.0;
    if (a == 0.0 && b < 0.0 && isfinite(b)) {
        lt_zero_division(at, "zero to a negative power");
    }
    if (a < 0.0 && isfinite(a) && isfinite(b) && b != floor(b)) {
        lt_value_error(at, "a negative number to a fractional power");
    }
    double r = pow(a, b);
    if (isinf(r) && isfinite(a) && isfinite(b)) {
        lt_panic(at, "OverflowError", "(34, 'Numerical result out of range')");
    }
    return r;
}

#include "lotml_text.c"
#include "lotml_list.c"

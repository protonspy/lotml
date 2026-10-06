/* The runtime's functions; see lotml.h. */

#include "lotml.h"

#include <stdarg.h>

#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <process.h>
#include <windows.h>
#else
#include <pthread.h>
#endif

/* Cells and output ------------------------------------------------------------------------- */

int64_t lt_live_cells = 0;
int64_t lt_allocated_cells = 0;

#ifdef LT_COUNT_CELLS
static void lt_count_cells(int64_t delta) {
#if LT_GNU
    __atomic_add_fetch(&lt_live_cells, delta, __ATOMIC_RELAXED);
    if (delta > 0) __atomic_add_fetch(&lt_allocated_cells, delta, __ATOMIC_RELAXED);
#else
    _InterlockedExchangeAdd64((volatile long long *)&lt_live_cells, delta);
    if (delta > 0) _InterlockedExchangeAdd64((volatile long long *)&lt_allocated_cells, delta);
#endif
}
#endif

static char lt_out[1 << 16];
static size_t lt_out_used = 0;

/* How many `parallel` calls are running: while any is, the output buffer is written under a
 * lock. Changed before the threads start and after they finish, so a reader never races it. */
static int64_t lt_tasks_running = 0;

#ifdef _WIN32
static SRWLOCK lt_out_lock = SRWLOCK_INIT;
static void lt_lock_out(void) {
    if (lt_tasks_running > 0) AcquireSRWLockExclusive(&lt_out_lock);
}
static void lt_unlock_out(bool locked) {
    if (locked) ReleaseSRWLockExclusive(&lt_out_lock);
}
#else
static pthread_mutex_t lt_out_lock = PTHREAD_MUTEX_INITIALIZER;
static void lt_lock_out(void) {
    if (lt_tasks_running > 0) pthread_mutex_lock(&lt_out_lock);
}
static void lt_unlock_out(bool locked) {
    if (locked) pthread_mutex_unlock(&lt_out_lock);
}
#endif

static void lt_flush_unlocked(void) {
    if (lt_out_used > 0) {
        fwrite(lt_out, 1, lt_out_used, stdout);
        lt_out_used = 0;
    }
    fflush(stdout);
}

void lt_flush(void) {
    bool locked = lt_tasks_running > 0;
    lt_lock_out();
    lt_flush_unlocked();
    lt_unlock_out(locked);
}

void lt_write(const char *bytes, size_t length) {
    bool locked = lt_tasks_running > 0;
    lt_lock_out();
    if (length >= sizeof lt_out) {
        lt_flush_unlocked();
        fwrite(bytes, 1, length, stdout);
    } else {
        if (lt_out_used + length > sizeof lt_out) lt_flush_unlocked();
        memcpy(lt_out + lt_out_used, bytes, length);
        lt_out_used += length;
    }
    lt_unlock_out(locked);
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
    fprintf(stderr, "lotml: %lld cells allocated\n", (long long)lt_allocated_cells);
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

/* A cell of `size` for a constructor: the reuse token `token`, of `token_size` bytes, when it is
 * large enough (R3.4), else a new one, the token freed. */
void *lt_reuse_or_alloc(void *token, size_t token_size, size_t size) {
    if (token != NULL && token_size >= size) {
        ((lt_cell *)token)->count = 1;
        ((lt_cell *)token)->aux = 0;
        return token;
    }
    if (token != NULL) lt_free(token);
    return lt_alloc(size);
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

/* A panic in a task of `parallel`: kept for the caller, which stops the program with the first
 * one once every task has finished. */
typedef struct lt_failure {
    bool failed;
    const char *kind;
    char message[512];
    lt_at at;
} lt_failure;

static LT_THREAD jmp_buf *lt_task_jump = NULL;
static LT_THREAD lt_failure *lt_task_failure = NULL;

void lt_panic(lt_at at, const char *kind, const char *message) {
    if (lt_task_jump != NULL) {
        lt_task_failure->failed = true;
        lt_task_failure->kind = kind;
        snprintf(lt_task_failure->message, sizeof lt_task_failure->message, "%s", message != NULL ? message : "");
        lt_task_failure->at = at;
        longjmp(*lt_task_jump, 1);
    }
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
    if (length == 0) return;
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

/* parallel --------------------------------------------------------------------------------- */

#define LT_TASK_THREADS 256

typedef struct lt_job {
    const lt_list *tasks;
    lt_task_fn run;
    char *out;
    size_t size;
    int64_t next;
    lt_failure *failures;
} lt_job;

static int64_t lt_next_task(lt_job *job) {
#if LT_GNU
    return __atomic_fetch_add(&job->next, 1, __ATOMIC_RELAXED);
#else
    return _InterlockedExchangeAdd64((volatile long long *)&job->next, 1);
#endif
}

static void lt_add_running(int64_t delta) {
#if LT_GNU
    __atomic_add_fetch(&lt_tasks_running, delta, __ATOMIC_SEQ_CST);
#else
    _InterlockedExchangeAdd64((volatile long long *)&lt_tasks_running, delta);
#endif
}

/* A worker: the next task not yet taken, until none is left; a panic caught and kept. */
static void lt_work(lt_job *job) {
    for (;;) {
        int64_t i = lt_next_task(job);
        if (i >= job->tasks->len) return;
        jmp_buf here;
        lt_task_jump = &here;
        lt_task_failure = &job->failures[i];
        if (setjmp(here) == 0) {
            lt_closure *task = *(lt_closure **)(job->tasks->data + (size_t)i * job->tasks->type->size);
            lt_inc(task);
            job->run(task, job->out + (size_t)i * job->size);
        }
        lt_task_jump = NULL;
        lt_task_failure = NULL;
    }
}

#ifdef _WIN32
static unsigned __stdcall lt_worker(void *job) {
    lt_work((lt_job *)job);
    return 0;
}
#else
static void *lt_worker(void *job) {
    lt_work((lt_job *)job);
    return NULL;
}
#endif

lt_list *lt_parallel(const lt_list *tasks, const lt_type *result, lt_task_fn run, lt_at at) {
    int64_t n = tasks->len;
    lt_list *results = lt_list_new(result, n);
    if (n == 0) return results;
    for (int64_t i = 0; i < n; i++) lt_type_closure.share(tasks->data + (size_t)i * tasks->type->size);
    lt_failure *failures = calloc((size_t)n, sizeof(lt_failure));
    if (failures == NULL) lt_panic(at, "MemoryError", "out of memory");
    memset(results->data, 0, (size_t)n * result->size);
    lt_job job = {tasks, run, results->data, result->size, 0, failures};
    int64_t workers = n < LT_TASK_THREADS ? n : LT_TASK_THREADS;
    /* jmp_buf and the failure the caller is itself a task of, if it is one */
    jmp_buf *outer_jump = lt_task_jump;
    lt_failure *outer_failure = lt_task_failure;
    lt_add_running(1);
#ifdef _WIN32
    HANDLE threads[LT_TASK_THREADS];
    int64_t started = 0;
    for (; started < workers; started++) {
        threads[started] = (HANDLE)_beginthreadex(NULL, 0, lt_worker, &job, 0, NULL);
        if (threads[started] == 0) break;
    }
    if (started == 0) lt_work(&job);
    for (int64_t i = 0; i < started; i++) {
        WaitForSingleObject(threads[i], INFINITE);
        CloseHandle(threads[i]);
    }
#else
    pthread_t threads[LT_TASK_THREADS];
    int64_t started = 0;
    for (; started < workers; started++) {
        if (pthread_create(&threads[started], NULL, lt_worker, &job) != 0) break;
    }
    if (started == 0) lt_work(&job);
    for (int64_t i = 0; i < started; i++) pthread_join(threads[i], NULL);
#endif
    lt_add_running(-1);
    lt_task_jump = outer_jump;
    lt_task_failure = outer_failure;
    results->len = n;
    for (int64_t i = 0; i < n; i++) {
        if (failures[i].failed) {
            lt_failure first = failures[i];
            free(failures);
            lt_panic(first.at, first.kind, first.message);
        }
    }
    free(failures);
    return results;
}

/* The prelude's arithmetic ---------------------------------------------------------------- */

/* `round(x)`: the nearest integer, a tie to the even one. */
int64_t lt_round_i64(double x, lt_at at) {
    if (isnan(x)) lt_value_error(at, "cannot convert float NaN to integer");
    if (isinf(x)) lt_panic(at, "OverflowError", "cannot convert float infinity to integer");
    return lt_f64_to_i64(nearbyint(x), at);
}

/* `round(x, digits)`: the double nearest to x rounded to `digits` decimal places, a tie to the
 * even digit, decided on the exact decimal value of x as CPython's dtoa does. */
double lt_round_f64(double x, int64_t digits) {
    if (!isfinite(x) || x == 0.0 || digits > 323) return x;
    if (digits < -308) return 0.0 * x;
    char text[1500];
    int n = snprintf(text, sizeof text, "%.1080f", fabs(x));
    char *point = strchr(text, '.');
    if (n <= 0 || point == NULL) return x;
    int64_t whole = (int64_t)(point - text);
    /* the digits without the point: the integer part, then the fraction */
    memmove(point, point + 1, strlen(point + 1) + 1);
    int64_t total = (int64_t)strlen(text);
    int64_t keep = whole + digits;
    if (keep < 0) return copysign(0.0, x);
    bool up = false;
    if (keep < total) {
        char d = text[keep];
        bool rest = false;
        for (int64_t i = keep + 1; i < total; i++) {
            if (text[i] != '0') {
                rest = true;
                break;
            }
        }
        bool odd = keep > 0 && (text[keep - 1] - '0') % 2 == 1;
        up = d > '5' || (d == '5' && (rest || odd));
    }
    char kept[1500];
    kept[0] = '0';
    memcpy(kept + 1, text, (size_t)(keep < total ? keep : total));
    int64_t len = 1 + (keep < total ? keep : total);
    if (up) {
        int64_t i = len - 1;
        while (kept[i] == '9') kept[i--] = '0';
        kept[i]++;
    }
    snprintf(kept + len, sizeof kept - (size_t)len, "e%lld", (long long)(keep < total ? -digits : whole - total));
    return copysign(strtod(kept, NULL), x);
}

/* a * b modulo m, for a and b below m: doubling, so no step leaves 64 bits. */
static uint64_t lt_mulmod(uint64_t a, uint64_t b, uint64_t m) {
    uint64_t r = 0;
    while (b > 0) {
        if (b & 1) r = r >= m - a ? r - (m - a) : r + a;
        b >>= 1;
        a = a >= m - a ? a - (m - a) : a + a;
    }
    return r;
}

/* `pow(base, exponent, modulus)`: a negative exponent raises the inverse; the result has the
 * modulus's sign, as Python's does. */
int64_t lt_pow_mod(int64_t base, int64_t exponent, int64_t modulus, lt_at at) {
    if (modulus == 0) lt_value_error(at, "pow() 3rd argument cannot be 0");
    uint64_t m = modulus < 0 ? (uint64_t)0 - (uint64_t)modulus : (uint64_t)modulus;
    uint64_t b = base >= 0 ? (uint64_t)base % m : (m - ((uint64_t)0 - (uint64_t)base) % m) % m;
    uint64_t e = exponent >= 0 ? (uint64_t)exponent : (uint64_t)0 - (uint64_t)exponent;
    if (exponent < 0) {
        uint64_t r0 = m, r1 = b, t0 = 0, t1 = 1 % m;
        while (r1 != 0) {
            uint64_t q = r0 / r1, r2 = r0 % r1;
            uint64_t qt = lt_mulmod(q % m, t1, m);
            uint64_t t2 = t0 >= qt ? t0 - qt : t0 + (m - qt);
            r0 = r1;
            r1 = r2;
            t0 = t1;
            t1 = t2;
        }
        if (r0 != 1) lt_value_error(at, "base is not invertible for the given modulus");
        b = t0;
    }
    uint64_t r = 1 % m;
    while (e > 0) {
        if (e & 1) r = lt_mulmod(r, b, m);
        e >>= 1;
        if (e > 0) b = lt_mulmod(b, b, m);
    }
    return modulus < 0 && r != 0 ? (int64_t)(r - m) : (int64_t)r;
}

int64_t lt_isqrt(int64_t n, lt_at at) {
    if (n < 0) lt_value_error(at, "isqrt() argument must be nonnegative");
    uint64_t r = (uint64_t)sqrt((double)n);
    while (r * r > (uint64_t)n) r--;
    while ((r + 1) * (r + 1) <= (uint64_t)n) r++;
    return (int64_t)r;
}

int64_t lt_gcd(int64_t a, int64_t b, lt_at at) {
    uint64_t x = a < 0 ? (uint64_t)0 - (uint64_t)a : (uint64_t)a;
    uint64_t y = b < 0 ? (uint64_t)0 - (uint64_t)b : (uint64_t)b;
    while (y != 0) {
        uint64_t t = x % y;
        x = y;
        y = t;
    }
    if (x > (uint64_t)INT64_MAX) lt_overflow(at, "int");
    return (int64_t)x;
}

/* math_1 of CPython's module: NaN from a number is a domain error, infinity from a finite
 * number a range error when the function can overflow. */
double lt_math_1(double (*f)(double), double x, bool can_overflow, lt_at at) {
    double r = f(x);
    if (isnan(r) && !isnan(x)) lt_value_error(at, "math domain error");
    if (isinf(r) && isfinite(x)) {
        if (can_overflow) lt_panic(at, "OverflowError", "math range error");
        lt_value_error(at, "math domain error");
    }
    return r;
}

double lt_math_2(double (*f)(double, double), double x, double y, lt_at at) {
    double r = f(x, y);
    if (isnan(r) && !isnan(x) && !isnan(y)) lt_value_error(at, "math domain error");
    if (isinf(r) && isfinite(x) && isfinite(y)) lt_panic(at, "OverflowError", "math range error");
    return r;
}

double lt_math_log(double (*f)(double), double x, lt_at at) {
    if (isnan(x) || x == INFINITY) return x;
    if (x <= 0.0) lt_value_error(at, "math domain error");
    return f(x);
}

double lt_math_pow(double x, double y, lt_at at) {
    if (isfinite(x) && isfinite(y)) {
        if (x == 0.0 && y < 0.0) lt_value_error(at, "math domain error");
        if (x < 0.0 && y != floor(y)) lt_value_error(at, "math domain error");
        double r = pow(x, y);
        if (isinf(r)) lt_panic(at, "OverflowError", "math range error");
        return r;
    }
    return pow(x, y);
}

int64_t lt_factorial(int64_t n, lt_at at) {
    if (n < 0) lt_value_error(at, "factorial() not defined for negative values");
    int64_t r = 1;
    for (int64_t i = 2; i <= n; i++) r = lt_mul_i64(r, i, at);
    return r;
}

static uint64_t lt_gcd_u64(uint64_t x, uint64_t y) {
    while (y != 0) {
        uint64_t t = x % y;
        x = y;
        y = t;
    }
    return x;
}

/* `math.comb(n, k)`: C(n, i + 1) from C(n, i) exactly, dividing before multiplying, so a step
 * leaves i64 only when the result does. */
int64_t lt_comb(int64_t n, int64_t k, lt_at at) {
    if (n < 0) lt_value_error(at, "n must be a non-negative integer");
    if (k < 0) lt_value_error(at, "k must be a non-negative integer");
    if (k > n) return 0;
    if (k > n - k) k = n - k;
    int64_t r = 1;
    for (int64_t i = 0; i < k; i++) {
        uint64_t d = (uint64_t)(i + 1);
        uint64_t g = lt_gcd_u64((uint64_t)r, d);
        int64_t factor = (int64_t)((uint64_t)(n - i) / (d / g));
        r = lt_mul_i64(r / (int64_t)g, factor, at);
    }
    return r;
}

int64_t lt_perm(int64_t n, int64_t k, lt_at at) {
    if (n < 0) lt_value_error(at, "n must be a non-negative integer");
    if (k < 0) lt_value_error(at, "k must be a non-negative integer");
    if (k > n) return 0;
    int64_t r = 1;
    for (int64_t i = 0; i < k; i++) r = lt_mul_i64(r, n - i, at);
    return r;
}

#include "lotml_text.c"
#include "lotml_list.c"
#include "lotml_dict.c"

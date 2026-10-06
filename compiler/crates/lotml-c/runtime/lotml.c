/* The runtime's functions; see lotml.h. */

#include "lotml.h"

#include <stdarg.h>

#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#include <intrin.h>
#endif

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

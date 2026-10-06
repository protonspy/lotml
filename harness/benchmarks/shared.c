#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <process.h>
#include <windows.h>
#else
#include <pthread.h>
#endif

typedef struct job {
    const int64_t *data;
    int64_t start, stop, sum;
} job;

static void part(job *j) {
    int64_t s = 0;
    for (int64_t rep = 0; rep < 5; rep++) {
        for (int64_t i = j->start; i < j->stop; i++) s += (j->data[i] + rep) % 7;
    }
    j->sum = s;
}

#ifdef _WIN32
static unsigned __stdcall run(void *j) {
    part(j);
    return 0;
}
#else
static void *run(void *j) {
    part(j);
    return NULL;
}
#endif

int main(void) {
    int64_t n = 8000000;
    int64_t *data = malloc((size_t)n * sizeof(int64_t));
    if (data == NULL) abort();
    for (int64_t i = 0; i < n; i++) data[i] = i;
    job jobs[4];
    for (int k = 0; k < 4; k++) jobs[k] = (job){data, k * 2000000, (k + 1) * 2000000, 0};
#ifdef _WIN32
    HANDLE threads[4];
    for (int k = 0; k < 4; k++) threads[k] = (HANDLE)_beginthreadex(NULL, 0, run, &jobs[k], 0, NULL);
    for (int k = 0; k < 4; k++) {
        WaitForSingleObject(threads[k], INFINITE);
        CloseHandle(threads[k]);
    }
#else
    pthread_t threads[4];
    for (int k = 0; k < 4; k++) pthread_create(&threads[k], NULL, run, &jobs[k]);
    for (int k = 0; k < 4; k++) pthread_join(threads[k], NULL);
#endif
    int64_t total = 0;
    for (int k = 0; k < 4; k++) total += jobs[k].sum;
    printf("%lld\n", (long long)total);
    free(data);
    return 0;
}

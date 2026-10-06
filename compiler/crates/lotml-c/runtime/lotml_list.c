/* Types of the runtime's own values, and lists. Included by lotml.c. */

/* Hashes, as CPython computes them ----------------------------------------------------------- */

#define LT_HASH_BITS 61
#define LT_HASH_MODULUS (((uint64_t)1 << LT_HASH_BITS) - 1)

int64_t lt_hash_u64(uint64_t value) {
    uint64_t h = value % LT_HASH_MODULUS;
    return (int64_t)h;
}

int64_t lt_hash_i64(int64_t value) {
    uint64_t magnitude = value < 0 ? (uint64_t)0 - (uint64_t)value : (uint64_t)value;
    int64_t h = (int64_t)(magnitude % LT_HASH_MODULUS);
    if (value < 0) h = -h;
    return h == -1 ? -2 : h;
}

/* _Py_HashDouble: a float equal to an integer hashes as that integer. */
int64_t lt_hash_f64(double value) {
    if (!isfinite(value)) {
        return isinf(value) ? (value > 0 ? 314159 : -314159) : 0;
    }
    int e;
    double m = frexp(value, &e);
    int sign = 1;
    if (m < 0) {
        sign = -1;
        m = -m;
    }
    uint64_t x = 0;
    while (m != 0.0) {
        x = ((x << 28) & LT_HASH_MODULUS) | x >> (LT_HASH_BITS - 28);
        m *= 268435456.0;
        e -= 28;
        uint64_t y = (uint64_t)m;
        m -= (double)y;
        x += y;
        if (x >= LT_HASH_MODULUS) x -= LT_HASH_MODULUS;
    }
    e = e >= 0 ? e % LT_HASH_BITS : LT_HASH_BITS - 1 - ((-1 - e) % LT_HASH_BITS);
    x = ((x << e) & LT_HASH_MODULUS) | x >> (LT_HASH_BITS - e);
    int64_t h = (int64_t)x * sign;
    return h == -1 ? -2 : h;
}

/* The hash of one part of a tuple: a type that has none cannot be in a set or a dict key. */
int64_t lt_hash_part(const lt_type *type, const void *value) {
    if (type->hash == NULL) lt_panic((lt_at){NULL, 0, NULL}, "TypeError", "unhashable type");
    return type->hash(value);
}

/* CPython's tuplehash: xxHash's mixing of each part's hash, then the length. */
int64_t lt_hash_tuple(const int64_t *lanes, int64_t n) {
    const uint64_t prime1 = 11400714785074694791ULL;
    const uint64_t prime2 = 14029467366897019727ULL;
    const uint64_t prime5 = 2870177450012600261ULL;
    uint64_t acc = prime5;
    for (int64_t i = 0; i < n; i++) {
        acc += (uint64_t)lanes[i] * prime2;
        acc = (acc << 31) | (acc >> 33);
        acc *= prime1;
    }
    acc += (uint64_t)n ^ (prime5 ^ 3527539UL);
    if (acc == (uint64_t)-1) return 1546275796;
    return (int64_t)acc;
}

/* The runtime's own types -------------------------------------------------------------------- */

void lt_unorderable(lt_at at, const char *type) {
    lt_panicf(at, "TypeError", "'<' not supported between instances of '%s' and '%s'", type, type);
}

#define LT_INT_TYPE(NAME, CTYPE, WIDE, PUT, HASH)                                                              \
    static bool lt_v_eq_##NAME(const void *a, const void *b) {                                                \
        return *(const CTYPE *)a == *(const CTYPE *)b;                                                       \
    }                                                                                                        \
    static int lt_v_cmp_##NAME(const void *a, const void *b, lt_at at) {                                      \
        (void)at;                                                                                            \
        CTYPE x = *(const CTYPE *)a, y = *(const CTYPE *)b;                                                  \
        return x < y ? -1 : x > y ? 1 : 0;                                                                   \
    }                                                                                                        \
    static int64_t lt_v_hash_##NAME(const void *a) {                                                           \
        return HASH((WIDE) * (const CTYPE *)a);                                                              \
    }                                                                                                        \
    static void lt_v_repr_##NAME(lt_buf *b, const void *a) {                                                   \
        PUT(b, (WIDE) * (const CTYPE *)a);                                                                   \
    }                                                                                                        \
    const lt_type lt_type_##NAME = {sizeof(CTYPE), NULL, NULL, lt_v_eq_##NAME, lt_v_cmp_##NAME, lt_v_hash_##NAME,  \
                                    lt_v_repr_##NAME, lt_v_repr_##NAME, NULL};

LT_INT_TYPE(i8, int8_t, int64_t, lt_buf_i64, lt_hash_i64)
LT_INT_TYPE(i16, int16_t, int64_t, lt_buf_i64, lt_hash_i64)
LT_INT_TYPE(i32, int32_t, int64_t, lt_buf_i64, lt_hash_i64)
LT_INT_TYPE(i64, int64_t, int64_t, lt_buf_i64, lt_hash_i64)
LT_INT_TYPE(u8, uint8_t, int64_t, lt_buf_i64, lt_hash_i64)
LT_INT_TYPE(u16, uint16_t, int64_t, lt_buf_i64, lt_hash_i64)
LT_INT_TYPE(u32, uint32_t, int64_t, lt_buf_i64, lt_hash_i64)
LT_INT_TYPE(u64, uint64_t, uint64_t, lt_buf_u64, lt_hash_u64)

static bool lt_eq_f64(const void *a, const void *b) {
    return *(const double *)a == *(const double *)b;
}
static int lt_cmp_f64(const void *a, const void *b, lt_at at) {
    (void)at;
    double x = *(const double *)a, y = *(const double *)b;
    return x < y ? -1 : x > y ? 1 : 0;
}
static int64_t lt_hash_f64_value(const void *a) {
    return lt_hash_f64(*(const double *)a);
}
static void lt_repr_f64(lt_buf *b, const void *a) {
    lt_buf_f64(b, *(const double *)a);
}
const lt_type lt_type_f64 = {sizeof(double), NULL, NULL, lt_eq_f64, lt_cmp_f64, lt_hash_f64_value, lt_repr_f64, lt_repr_f64, NULL};

static bool lt_eq_bool(const void *a, const void *b) {
    return *(const bool *)a == *(const bool *)b;
}
static int lt_cmp_bool(const void *a, const void *b, lt_at at) {
    (void)at;
    return (int)*(const bool *)a - (int)*(const bool *)b;
}
static int64_t lt_hash_bool(const void *a) {
    return *(const bool *)a ? 1 : 0;
}
static void lt_repr_bool(lt_buf *b, const void *a) {
    lt_buf_bool(b, *(const bool *)a);
}
const lt_type lt_type_bool = {sizeof(bool), NULL, NULL, lt_eq_bool, lt_cmp_bool, lt_hash_bool, lt_repr_bool, lt_repr_bool, NULL};

static bool lt_eq_none(const void *a, const void *b) {
    (void)a;
    (void)b;
    return true;
}
static int lt_cmp_none(const void *a, const void *b, lt_at at) {
    (void)a;
    (void)b;
    lt_unorderable(at, "NoneType");
}
static int64_t lt_hash_none(const void *a) {
    (void)a;
    return (int64_t)0xFCA86420;
}
static void lt_repr_none(lt_buf *b, const void *a) {
    (void)a;
    lt_buf_puts(b, "None");
}
const lt_type lt_type_none = {sizeof(uint8_t), NULL, NULL, lt_eq_none, lt_cmp_none, lt_hash_none, lt_repr_none, lt_repr_none, NULL};

static void lt_inc_str(void *a) {
    if (*(lt_str **)a != NULL) lt_inc(*(lt_str **)a);
}
static void lt_dec_str(void *a) {
    lt_str_drop(*(lt_str **)a);
}
static bool lt_eq_str(const void *a, const void *b) {
    return lt_str_eq(*(lt_str *const *)a, *(lt_str *const *)b);
}
static int lt_cmp_str(const void *a, const void *b, lt_at at) {
    (void)at;
    return lt_str_compare(*(lt_str *const *)a, *(lt_str *const *)b);
}
static void lt_repr_str(lt_buf *b, const void *a) {
    lt_buf_str_repr(b, *(lt_str *const *)a);
}
static void lt_str_str(lt_buf *b, const void *a) {
    lt_buf_str(b, *(lt_str *const *)a);
}
static void lt_share_cell(void *a) {
    lt_cell *c = *(lt_cell **)a;
    if (lt_count_of(c) > 0) c->count = -c->count;
}
static int64_t lt_hash_str_value(const void *value);
const lt_type lt_type_str = {sizeof(lt_str *), lt_inc_str, lt_dec_str, lt_eq_str, lt_cmp_str, lt_hash_str_value, lt_repr_str, lt_str_str, lt_share_cell};

void lt_buf_value(lt_buf *b, const lt_type *type, const void *value) {
    type->str(b, value);
}

lt_str *lt_str_of_value(const lt_type *type, const void *value) {
    lt_buf b = LT_BUF;
    type->str(&b, value);
    return lt_str_from_buf(&b);
}

void lt_format_value(lt_buf *b, const lt_type *type, const void *value, const char *spec, size_t size, lt_at at) {
    (void)spec;
    if (size != 0) lt_panic(at, "TypeError", "unsupported format string passed to this value's __format__");
    type->str(b, value);
}

/* Lists ------------------------------------------------------------------------------------ */

void lt_index_error(lt_at at, const char *message) {
    lt_panic(at, "IndexError", message);
}

lt_list *lt_list_new(const lt_type *type, int64_t cap) {
    lt_list *l = lt_alloc(sizeof(lt_list));
    l->len = 0;
    l->cap = cap > 0 ? cap : 0;
    l->type = type;
    l->data = cap > 0 ? malloc((size_t)cap * type->size) : NULL;
    if (cap > 0 && l->data == NULL) lt_panic((lt_at){NULL, 0, NULL}, "MemoryError", "out of memory");
    return l;
}

#define LT_AT(l, i) ((l)->data + (size_t)(i) * (l)->type->size)

void lt_list_drop(lt_list *l) {
    if (l == NULL || !lt_dec(l)) return;
    if (l->type->dec != NULL) {
        for (int64_t i = 0; i < l->len; i++) l->type->dec(LT_AT(l, i));
    }
    free(l->data);
    lt_free(l);
}

void lt_list_inc(void *value) {
    if (*(lt_list **)value != NULL) lt_inc(*(lt_list **)value);
}

void lt_list_dec(void *value) {
    lt_list_drop(*(lt_list **)value);
}

static void lt_list_grow(lt_list *l, int64_t need) {
    if (need <= l->cap) return;
    int64_t cap = l->cap < 4 ? 4 : l->cap * 2;
    while (cap < need) cap *= 2;
    char *data = realloc(l->data, (size_t)cap * l->type->size);
    if (data == NULL) lt_panic((lt_at){NULL, 0, NULL}, "MemoryError", "out of memory");
    l->data = data;
    l->cap = cap;
}

/* A copy of `l` holding the same elements, each counted once more. */
lt_list *lt_list_copy(const lt_list *l) {
    lt_list *c = lt_list_new(l->type, l->len);
    if (l->len > 0) memcpy(c->data, l->data, (size_t)l->len * l->type->size);
    c->len = l->len;
    if (l->type->inc != NULL) {
        for (int64_t i = 0; i < c->len; i++) l->type->inc(LT_AT(c, i));
    }
    return c;
}

/* Make the list in `slot` one that only this slot holds, copying it when another does (R3.5). */
void lt_list_unique(lt_list **slot) {
    if (lt_unique(*slot)) return;
    lt_list *c = lt_list_copy(*slot);
    lt_list_drop(*slot);
    *slot = c;
}

void *lt_list_slot(lt_list **slot, int64_t index, lt_at at) {
    lt_list_unique(slot);
    return LT_AT(*slot, lt_index((*slot)->len, index, at));
}

void lt_list_push(lt_list **slot, const void *value) {
    lt_list_unique(slot);
    lt_list *l = *slot;
    lt_list_grow(l, l->len + 1);
    memcpy(LT_AT(l, l->len), value, l->type->size);
    l->len++;
}

void lt_list_extend(lt_list **slot, const lt_list *other) {
    lt_list_unique(slot);
    lt_list *l = *slot;
    int64_t n = other->len;
    lt_list_grow(l, l->len + n);
    if (n > 0) memcpy(LT_AT(l, l->len), other->data, (size_t)n * l->type->size);
    if (l->type->inc != NULL) {
        for (int64_t i = 0; i < n; i++) l->type->inc(LT_AT(l, l->len + i));
    }
    l->len += n;
}

void lt_list_insert(lt_list **slot, int64_t index, const void *value) {
    lt_list_unique(slot);
    lt_list *l = *slot;
    if (index < 0) {
        index += l->len;
        if (index < 0) index = 0;
    }
    if (index > l->len) index = l->len;
    lt_list_grow(l, l->len + 1);
    memmove(LT_AT(l, index + 1), LT_AT(l, index), (size_t)(l->len - index) * l->type->size);
    memcpy(LT_AT(l, index), value, l->type->size);
    l->len++;
}

static int64_t lt_list_find(const lt_list *l, const void *value) {
    for (int64_t i = 0; i < l->len; i++) {
        if (l->type->eq(LT_AT(l, i), value)) return i;
    }
    return -1;
}

void lt_list_remove(lt_list **slot, const void *value, lt_at at) {
    int64_t at_index = lt_list_find(*slot, value);
    if (at_index < 0) lt_value_error(at, "list.remove(x): x not in list");
    lt_list_unique(slot);
    lt_list *l = *slot;
    if (l->type->dec != NULL) l->type->dec(LT_AT(l, at_index));
    memmove(LT_AT(l, at_index), LT_AT(l, at_index + 1), (size_t)(l->len - at_index - 1) * l->type->size);
    l->len--;
}

void lt_list_clear(lt_list **slot) {
    if (!lt_unique(*slot)) {
        const lt_type *type = (*slot)->type;
        lt_list_drop(*slot);
        *slot = lt_list_new(type, 0);
        return;
    }
    lt_list *l = *slot;
    if (l->type->dec != NULL) {
        for (int64_t i = 0; i < l->len; i++) l->type->dec(LT_AT(l, i));
    }
    l->len = 0;
}

static void lt_swap(char *a, char *b, size_t size) {
    char tmp[64];
    while (size > 0) {
        size_t n = size < sizeof tmp ? size : sizeof tmp;
        memcpy(tmp, a, n);
        memcpy(a, b, n);
        memcpy(b, tmp, n);
        a += n;
        b += n;
        size -= n;
    }
}

void lt_list_reverse(lt_list **slot) {
    lt_list_unique(slot);
    lt_list *l = *slot;
    for (int64_t i = 0, j = l->len - 1; i < j; i++, j--) lt_swap(LT_AT(l, i), LT_AT(l, j), l->type->size);
}

/* A stable merge sort of `n` elements of `size` at `data`, by `type->cmp`, descending when
 * `reverse` — every comparison reversed, so equal elements keep their order as Python's do. */
static void lt_merge_sort(char *data, char *scratch, int64_t n, const lt_type *type, bool reverse, lt_at at) {
    size_t size = type->size;
    if (n < 2) return;
    if (n <= 8) {
        for (int64_t i = 1; i < n; i++) {
            memcpy(scratch, data + (size_t)i * size, size);
            int64_t j = i - 1;
            while (j >= 0) {
                int c = type->cmp(scratch, data + (size_t)j * size, at);
                if (reverse ? c <= 0 : c >= 0) break;
                memcpy(data + (size_t)(j + 1) * size, data + (size_t)j * size, size);
                j--;
            }
            memcpy(data + (size_t)(j + 1) * size, scratch, size);
        }
        return;
    }
    int64_t half = n / 2;
    lt_merge_sort(data, scratch, half, type, reverse, at);
    lt_merge_sort(data + (size_t)half * size, scratch, n - half, type, reverse, at);
    memcpy(scratch, data, (size_t)half * size);
    int64_t i = 0, j = half, k = 0;
    while (i < half && j < n) {
        int c = type->cmp(data + (size_t)j * size, scratch + (size_t)i * size, at);
        bool take_right = reverse ? c > 0 : c < 0;
        if (take_right) {
            memcpy(data + (size_t)k * size, data + (size_t)j * size, size);
            j++;
        } else {
            memcpy(data + (size_t)k * size, scratch + (size_t)i * size, size);
            i++;
        }
        k++;
    }
    if (i < half) memcpy(data + (size_t)k * size, scratch + (size_t)i * size, (size_t)(half - i) * size);
}

void lt_list_sort(lt_list **slot, bool reverse, lt_at at) {
    lt_list_unique(slot);
    lt_list *l = *slot;
    if (l->len < 2) return;
    char *scratch = malloc((size_t)(l->len / 2 + 1) * l->type->size);
    lt_merge_sort(l->data, scratch, l->len, l->type, reverse, at);
    free(scratch);
}

/* The order of `n` keys, a stable merge sort of their indices by `keys->type->cmp`. */
static void lt_sort_indices(int64_t *order, int64_t *scratch, int64_t n, const lt_list *keys, bool reverse, lt_at at) {
    if (n < 2) return;
    int64_t half = n / 2;
    lt_sort_indices(order, scratch, half, keys, reverse, at);
    lt_sort_indices(order + half, scratch, n - half, keys, reverse, at);
    memcpy(scratch, order, (size_t)half * sizeof(int64_t));
    int64_t i = 0, j = half, k = 0;
    while (i < half && j < n) {
        int c = keys->type->cmp(LT_AT(keys, order[j]), LT_AT(keys, scratch[i]), at);
        if (reverse ? c > 0 : c < 0) {
            order[k++] = order[j++];
        } else {
            order[k++] = scratch[i++];
        }
    }
    while (i < half) order[k++] = scratch[i++];
}

void lt_list_sort_by_keys(lt_list **slot, const lt_list *keys, bool reverse, lt_at at) {
    lt_list_unique(slot);
    lt_list *l = *slot;
    if (l->len < 2) return;
    int64_t *order = malloc(sizeof(int64_t) * (size_t)l->len);
    int64_t *scratch = malloc(sizeof(int64_t) * (size_t)(l->len / 2 + 1));
    for (int64_t i = 0; i < l->len; i++) order[i] = i;
    lt_sort_indices(order, scratch, l->len, keys, reverse, at);
    char *sorted = malloc((size_t)l->len * l->type->size);
    for (int64_t i = 0; i < l->len; i++) memcpy(sorted + (size_t)i * l->type->size, LT_AT(l, order[i]), l->type->size);
    free(l->data);
    l->data = sorted;
    l->cap = l->len;
    free(order);
    free(scratch);
}

lt_list *lt_list_sorted(const lt_list *l, bool reverse, lt_at at) {
    lt_list *c = lt_list_copy(l);
    lt_list_sort(&c, reverse, at);
    return c;
}

lt_list *lt_list_reversed(const lt_list *l) {
    lt_list *c = lt_list_copy(l);
    lt_list_reverse(&c);
    return c;
}

lt_list *lt_list_slice(const lt_list *l, bool has_lo, int64_t lo, bool has_hi, int64_t hi, bool has_step, int64_t step, lt_at at) {
    int64_t start, count, by;
    lt_slice_indices(l->len, has_lo, lo, has_hi, hi, has_step, step, &start, &count, &by, at);
    lt_list *c = lt_list_new(l->type, count);
    for (int64_t i = 0; i < count; i++) {
        memcpy(LT_AT(c, i), LT_AT(l, start + i * by), l->type->size);
        if (l->type->inc != NULL) l->type->inc(LT_AT(c, i));
    }
    c->len = count;
    return c;
}

lt_list *lt_list_concat(const lt_list *a, const lt_list *b) {
    lt_list *c = lt_list_copy(a);
    lt_list_extend(&c, b);
    return c;
}

lt_list *lt_list_repeat(const lt_list *l, int64_t times, lt_at at) {
    if (times <= 0) return lt_list_new(l->type, 0);
    if (l->len > 0 && times > INT64_MAX / l->len) lt_panic(at, "MemoryError", "the repeated list is too long");
    lt_list *c = lt_list_new(l->type, l->len * times);
    for (int64_t t = 0; t < times; t++) lt_list_extend(&c, l);
    return c;
}

bool lt_list_index(const lt_list *l, const void *value, int64_t *out) {
    *out = lt_list_find(l, value);
    if (*out < 0) {
        *out = 0;
        return false;
    }
    return true;
}

/* `xs.pop()` and `xs.pop(i)`: the element taken out, its count with it. */
bool lt_list_pop(lt_list **slot, bool has_index, int64_t index, void *out, lt_at at) {
    memset(out, 0, (*slot)->type->size);
    if ((*slot)->len == 0) return false;
    lt_list_unique(slot);
    lt_list *l = *slot;
    int64_t i = has_index ? lt_index(l->len, index, at) : l->len - 1;
    memcpy(out, LT_AT(l, i), l->type->size);
    memmove(LT_AT(l, i), LT_AT(l, i + 1), (size_t)(l->len - i - 1) * l->type->size);
    l->len--;
    return true;
}

/* `xs.last()`: the last element, counted once more. */
bool lt_list_last(const lt_list *l, void *out) {
    memset(out, 0, l->type->size);
    if (l->len == 0) return false;
    memcpy(out, LT_AT(l, l->len - 1), l->type->size);
    if (l->type->inc != NULL) l->type->inc(out);
    return true;
}

bool lt_list_contains(const lt_list *l, const void *value) {
    return lt_list_find(l, value) >= 0;
}

int64_t lt_list_count(const lt_list *l, const void *value) {
    int64_t n = 0;
    for (int64_t i = 0; i < l->len; i++) {
        if (l->type->eq(LT_AT(l, i), value)) n++;
    }
    return n;
}

/* The first smallest element of `l`, or the first largest: `min` and `max`. */
const void *lt_list_extreme(const lt_list *l, bool max, lt_at at) {
    if (l->len == 0) lt_value_error(at, max ? "max() iterable argument is empty" : "min() iterable argument is empty");
    const char *best = l->data;
    for (int64_t i = 1; i < l->len; i++) {
        int c = l->type->cmp(LT_AT(l, i), best, at);
        if (max ? c > 0 : c < 0) best = LT_AT(l, i);
    }
    return best;
}

bool lt_list_any(const lt_list *l) {
    for (int64_t i = 0; i < l->len; i++) {
        if (((const bool *)l->data)[i]) return true;
    }
    return false;
}

bool lt_list_all(const lt_list *l) {
    for (int64_t i = 0; i < l->len; i++) {
        if (!((const bool *)l->data)[i]) return false;
    }
    return true;
}

int64_t lt_sum_i64(const lt_list *l, lt_at at) {
    int64_t total = 0;
    size_t size = l->type->size;
    for (int64_t i = 0; i < l->len; i++) {
        int64_t x;
        const char *p = LT_AT(l, i);
        if (size == 8) x = *(const int64_t *)p;
        else if (size == 4) x = l->type == &lt_type_u32 ? (int64_t) * (const uint32_t *)p : (int64_t) * (const int32_t *)p;
        else if (size == 2) x = l->type == &lt_type_u16 ? (int64_t) * (const uint16_t *)p : (int64_t) * (const int16_t *)p;
        else x = l->type == &lt_type_u8 ? (int64_t) * (const uint8_t *)p : (int64_t) * (const int8_t *)p;
        total = lt_add_i64(total, x, at);
    }
    return total;
}

uint64_t lt_sum_u64(const lt_list *l, lt_at at) {
    uint64_t total = 0;
    for (int64_t i = 0; i < l->len; i++) total = lt_add_u64(total, ((const uint64_t *)l->data)[i], at);
    return total;
}

/* CPython's sum of floats since 3.12: Neumaier's compensated summation, the first element added
 * to the int 0. */
double lt_sum_f64(const lt_list *l) {
    const double *xs = (const double *)l->data;
    if (l->len == 0) return 0.0;
    double total = 0.0 + xs[0];
    double c = 0.0;
    for (int64_t i = 1; i < l->len; i++) {
        double x = xs[i];
        double t = total + x;
        if (fabs(total) >= fabs(x)) {
            c += (total - t) + x;
        } else {
            c += (x - t) + total;
        }
        total = t;
    }
    if (c != 0.0 && isfinite(c)) total += c;
    return total;
}

lt_list *lt_range_list(int64_t start, int64_t stop, int64_t step, lt_at at) {
    lt_range r = lt_range_new(start, stop, step, at);
    lt_list *l = lt_list_new(&lt_type_i64, 0);
    int64_t value;
    while (lt_range_step(&r, &value)) lt_list_push(&l, &value);
    return l;
}

static void lt_list_push_str(lt_list *l, lt_str *s) {
    lt_list_grow(l, l->len + 1);
    ((lt_str **)l->data)[l->len++] = s;
}

lt_list *lt_str_chars(const lt_str *s) {
    lt_list *l = lt_list_new(&lt_type_str, s->length);
    for (int64_t i = 0; i < s->size;) {
        int n;
        lt_decode(s->bytes + i, &n);
        lt_list_push_str(l, lt_str_new(s->bytes + i, n));
        i += n;
    }
    return l;
}

/* `s.split(sep, maxsplit)`: on runs of whitespace when `sep` is NULL, else on each `sep`. */
lt_list *lt_str_split(const lt_str *s, const lt_str *sep, int64_t maxsplit, lt_at at) {
    lt_list *l = lt_list_new(&lt_type_str, 0);
    if (sep == NULL) {
        int64_t i = 0;
        int64_t splits = 0;
        while (i < s->size) {
            int n;
            while (i < s->size && lt_is_space(lt_decode(s->bytes + i, &n))) i += n;
            if (i >= s->size) break;
            if (maxsplit >= 0 && splits >= maxsplit) {
                int64_t end = s->size;
                while (end > i) {
                    int64_t start = end - 1;
                    while (start > i && ((unsigned char)s->bytes[start] & 0xC0) == 0x80) start--;
                    if (!lt_is_space(lt_decode(s->bytes + start, &n))) break;
                    end = start;
                }
                lt_list_push_str(l, lt_str_new(s->bytes + i, end - i));
                return l;
            }
            int64_t start = i;
            while (i < s->size && !lt_is_space(lt_decode(s->bytes + i, &n))) i += n;
            lt_list_push_str(l, lt_str_new(s->bytes + start, i - start));
            splits++;
        }
        return l;
    }
    if (sep->size == 0) lt_value_error(at, "empty separator");
    int64_t from = 0;
    int64_t splits = 0;
    while (maxsplit < 0 || splits < maxsplit) {
        int64_t hit = lt_find_bytes(s, sep, from);
        if (hit < 0) break;
        lt_list_push_str(l, lt_str_new(s->bytes + from, hit - from));
        from = hit + sep->size;
        splits++;
    }
    lt_list_push_str(l, lt_str_new(s->bytes + from, s->size - from));
    return l;
}

static bool lt_is_line_break(int32_t c) {
    return c == '\n' || c == '\r' || c == 0x0B || c == 0x0C || c == 0x1C || c == 0x1D || c == 0x1E || c == 0x85 ||
           c == 0x2028 || c == 0x2029;
}

lt_list *lt_str_splitlines(const lt_str *s) {
    lt_list *l = lt_list_new(&lt_type_str, 0);
    int64_t start = 0;
    for (int64_t i = 0; i < s->size;) {
        int n;
        int32_t c = lt_decode(s->bytes + i, &n);
        if (lt_is_line_break(c)) {
            lt_list_push_str(l, lt_str_new(s->bytes + start, i - start));
            i += n;
            if (c == '\r' && i < s->size && s->bytes[i] == '\n') i++;
            start = i;
        } else {
            i += n;
        }
    }
    if (start < s->size) lt_list_push_str(l, lt_str_new(s->bytes + start, s->size - start));
    return l;
}

lt_str *lt_str_join(const lt_str *sep, const lt_list *parts) {
    lt_buf b = LT_BUF;
    for (int64_t i = 0; i < parts->len; i++) {
        if (i > 0) lt_buf_str(&b, sep);
        lt_buf_str(&b, ((lt_str *const *)parts->data)[i]);
    }
    return lt_str_from_buf(&b);
}

/* One part of `s.partition(sep)`: 0 before the first `sep`, 1 the separator, 2 after it. */
lt_str *lt_str_partition_part(const lt_str *s, const lt_str *sep, int which, lt_at at) {
    if (sep->size == 0) lt_value_error(at, "empty separator");
    int64_t hit = lt_find_bytes(s, sep, 0);
    if (hit < 0) return which == 0 ? lt_str_new(s->bytes, s->size) : lt_str_new("", 0);
    if (which == 0) return lt_str_new(s->bytes, hit);
    if (which == 1) return lt_str_new(sep->bytes, sep->size);
    return lt_str_new(s->bytes + hit + sep->size, s->size - hit - sep->size);
}

static bool lt_eq_list(const void *a, const void *b) {
    const lt_list *x = *(lt_list *const *)a;
    const lt_list *y = *(lt_list *const *)b;
    if (x == y) return true;
    if (x->len != y->len) return false;
    for (int64_t i = 0; i < x->len; i++) {
        if (!x->type->eq(LT_AT(x, i), LT_AT(y, i))) return false;
    }
    return true;
}

/* Lists order by their first differing element, then by length, as Python's do. */
static int lt_cmp_list(const void *a, const void *b, lt_at at) {
    const lt_list *x = *(lt_list *const *)a;
    const lt_list *y = *(lt_list *const *)b;
    int64_t n = x->len < y->len ? x->len : y->len;
    for (int64_t i = 0; i < n; i++) {
        if (!x->type->eq(LT_AT(x, i), LT_AT(y, i))) return x->type->cmp(LT_AT(x, i), LT_AT(y, i), at);
    }
    return x->len < y->len ? -1 : x->len > y->len ? 1 : 0;
}

static void lt_repr_list(lt_buf *b, const void *a) {
    const lt_list *l = *(lt_list *const *)a;
    lt_buf_put(b, "[", 1);
    for (int64_t i = 0; i < l->len; i++) {
        if (i > 0) lt_buf_put(b, ", ", 2);
        l->type->repr(b, LT_AT(l, i));
    }
    lt_buf_put(b, "]", 1);
}

static void lt_share_list(void *a) {
    lt_list *l = *(lt_list **)a;
    if (lt_count_of(&l->cell) <= 0) return;
    l->cell.count = -l->cell.count;
    if (l->type->share != NULL) {
        for (int64_t i = 0; i < l->len; i++) l->type->share(LT_AT(l, i));
    }
}

void lt_closure_drop(lt_closure *c) {
    if (c == NULL || !lt_dec(c)) return;
    if (c->drop != NULL) c->drop(c);
    lt_free(c);
}

static void lt_inc_closure(void *value) {
    if (*(lt_closure **)value != NULL) lt_inc(*(lt_closure **)value);
}

static void lt_dec_closure(void *value) {
    lt_closure_drop(*(lt_closure **)value);
}

static bool lt_eq_closure(const void *a, const void *b) {
    return *(lt_closure *const *)a == *(lt_closure *const *)b;
}

static int lt_cmp_closure(const void *a, const void *b, lt_at at) {
    (void)a;
    (void)b;
    lt_unorderable(at, "function");
}

static void lt_repr_closure(lt_buf *b, const void *a) {
    (void)a;
    lt_buf_puts(b, "<function>");
}

/* A closure marked shared marks what it captured too. */
static void lt_share_closure(void *a) {
    lt_closure *c = *(lt_closure **)a;
    if (c == NULL || lt_count_of(&c->cell) <= 0) return;
    c->cell.count = -c->cell.count;
    if (c->share != NULL) c->share(c);
}

const lt_type lt_type_closure = {sizeof(lt_closure *), lt_inc_closure, lt_dec_closure, lt_eq_closure, lt_cmp_closure, NULL,
                                 lt_repr_closure, lt_repr_closure, lt_share_closure};

const lt_type lt_type_list = {sizeof(lt_list *), lt_list_inc, lt_list_dec, lt_eq_list, lt_cmp_list, NULL, lt_repr_list, lt_repr_list, lt_share_list};

/* Heaps ----------------------------------------------------------------------------------- */

static bool lt_heap_less(const lt_list *l, int64_t a, int64_t b, lt_at at) {
    return l->type->cmp(LT_AT(l, a), LT_AT(l, b), at) < 0;
}

/* Room for one element too large for the stack's buffer. */
static char *lt_heap_scratch(size_t size) {
    char *p = malloc(size);
    if (p == NULL) abort();
    return p;
}

static void lt_heap_swap(lt_list *l, int64_t a, int64_t b) {
    char tmp[256];
    size_t size = l->type->size;
    char *t = size <= sizeof tmp ? tmp : lt_heap_scratch(size);
    memcpy(t, LT_AT(l, a), size);
    memcpy(LT_AT(l, a), LT_AT(l, b), size);
    memcpy(LT_AT(l, b), t, size);
    if (t != tmp) free(t);
}

/* heapq's siftdown: the element at pos moved up past every parent greater than it. */
static void lt_heap_down(lt_list *l, int64_t start, int64_t pos, lt_at at) {
    while (pos > start) {
        int64_t parent = (pos - 1) >> 1;
        if (!lt_heap_less(l, pos, parent, at)) break;
        lt_heap_swap(l, parent, pos);
        pos = parent;
    }
}

/* heapq's siftup: the smaller child moved up until a leaf, then the element sifted down. */
static void lt_heap_up(lt_list *l, int64_t pos, lt_at at) {
    int64_t end = l->len, start = pos, limit = end >> 1;
    while (pos < limit) {
        int64_t child = 2 * pos + 1;
        if (child + 1 < end && !lt_heap_less(l, child, child + 1, at)) child++;
        lt_heap_swap(l, child, pos);
        pos = child;
    }
    lt_heap_down(l, start, pos, at);
}

static int64_t lt_keep_top_bit(int64_t n) {
    int i = 0;
    while (n > 1) {
        n >>= 1;
        i++;
    }
    return n << i;
}

void lt_heapify(lt_list **slot, lt_at at) {
    lt_list_unique(slot);
    lt_list *l = *slot;
    int64_t n = l->len;
    if (n <= 2500) {
        for (int64_t i = (n >> 1) - 1; i >= 0; i--) lt_heap_up(l, i, at);
        return;
    }
    /* CPython's cache-friendly order for a large list, which leaves the same arrangement */
    int64_t m = n >> 1, leftmost = lt_keep_top_bit(m + 1) - 1, mhalf = m >> 1;
    for (int64_t i = leftmost - 1; i >= mhalf; i--) {
        for (int64_t j = i;; j >>= 1) {
            lt_heap_up(l, j, at);
            if (!(j & 1)) break;
        }
    }
    for (int64_t i = m - 1; i >= leftmost; i--) {
        for (int64_t j = i;; j >>= 1) {
            lt_heap_up(l, j, at);
            if (!(j & 1)) break;
        }
    }
}

void lt_heap_push(lt_list **slot, const void *value, lt_at at) {
    lt_list_push(slot, value);
    lt_heap_down(*slot, 0, (*slot)->len - 1, at);
}

bool lt_heap_pop(lt_list **slot, void *out, lt_at at) {
    if (!lt_list_pop(slot, false, 0, out, at)) return false;
    lt_list *l = *slot;
    if (l->len == 0) return true;
    size_t size = l->type->size;
    char tmp[256];
    char *t = size <= sizeof tmp ? tmp : lt_heap_scratch(size);
    memcpy(t, LT_AT(l, 0), size);
    memcpy(LT_AT(l, 0), out, size);
    memcpy(out, t, size);
    if (t != tmp) free(t);
    lt_heap_up(l, 0, at);
    return true;
}

/* `heap.peek()`: the smallest element, counted once more. */
bool lt_heap_peek(const lt_list *l, void *out) {
    memset(out, 0, l->type->size);
    if (l->len == 0) return false;
    memcpy(out, LT_AT(l, 0), l->type->size);
    if (l->type->inc != NULL) l->type->inc(out);
    return true;
}

/* Heaps are equal when they hold the same elements, in whatever order. */
static bool lt_eq_heap(const void *a, const void *b) {
    const lt_list *x = *(lt_list *const *)a;
    const lt_list *y = *(lt_list *const *)b;
    if (x == y) return true;
    if (x->len != y->len) return false;
    lt_at at = {NULL, 0, NULL};
    lt_list *sx = lt_list_sorted(x, false, at), *sy = lt_list_sorted(y, false, at);
    bool same = lt_type_list.eq(&sx, &sy);
    lt_list_drop(sx);
    lt_list_drop(sy);
    return same;
}

static int lt_cmp_heap(const void *a, const void *b, lt_at at) {
    (void)a;
    (void)b;
    lt_unorderable(at, "Heap");
}

static void lt_repr_heap(lt_buf *b, const void *a) {
    (void)a;
    lt_buf_puts(b, "<Heap>");
}

const lt_type lt_type_heap = {sizeof(lt_list *), lt_list_inc, lt_list_dec, lt_eq_heap, lt_cmp_heap, NULL, lt_repr_heap, lt_repr_heap, lt_share_list};

/* `hash(x)`: what a set would file x under; a value that has none stops the program. */
int64_t lt_hash_value(const lt_type *type, const void *value, lt_at at) {
    if (type->hash == NULL) lt_panic(at, "TypeError", "unhashable type");
    return type->hash(value);
}

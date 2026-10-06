/* Dicts and sets, and the hash of a string. Included by lotml.c.
 *
 * A dict keeps its entries in insertion order behind an index table, as CPython's does. A set is
 * CPython's open-addressing table entry for entry — its initial size, probe sequence, growth and
 * dummies — so that iterating or printing one gives CPython's order (R1.3). */

/* The hash of a string -------------------------------------------------------------------- */

#define LT_ROTATE(x, b) (uint64_t)(((x) << (b)) | ((x) >> (64 - (b))))
#define LT_HALF_ROUND(a, b, c, d, s, t) \
    a += b;                             \
    c += d;                             \
    b = LT_ROTATE(b, s) ^ a;            \
    d = LT_ROTATE(d, t) ^ c;            \
    a = LT_ROTATE(a, 32);
#define LT_SINGLE_ROUND(v0, v1, v2, v3)      \
    LT_HALF_ROUND(v0, v1, v2, v3, 13, 16); \
    LT_HALF_ROUND(v2, v1, v0, v3, 17, 21);

/* SipHash-1-3 with the key CPython uses under PYTHONHASHSEED=0: zero. */
static uint64_t lt_siphash13(const uint8_t *in, int64_t size) {
    uint64_t b = (uint64_t)size << 56;
    uint64_t v0 = 0x736f6d6570736575ULL;
    uint64_t v1 = 0x646f72616e646f6dULL;
    uint64_t v2 = 0x6c7967656e657261ULL;
    uint64_t v3 = 0x7465646279746573ULL;
    while (size >= 8) {
        uint64_t m = 0;
        for (int i = 0; i < 8; i++) m |= (uint64_t)in[i] << (8 * i);
        in += 8;
        size -= 8;
        v3 ^= m;
        LT_SINGLE_ROUND(v0, v1, v2, v3);
        v0 ^= m;
    }
    uint64_t t = 0;
    for (int64_t i = 0; i < size; i++) t |= (uint64_t)in[i] << (8 * i);
    b |= t;
    v3 ^= b;
    LT_SINGLE_ROUND(v0, v1, v2, v3);
    v0 ^= b;
    v2 ^= 0xff;
    LT_SINGLE_ROUND(v0, v1, v2, v3);
    LT_SINGLE_ROUND(v0, v1, v2, v3);
    LT_SINGLE_ROUND(v0, v1, v2, v3);
    return (v0 ^ v1) ^ (v2 ^ v3);
}

/* hash(s) as CPython computes it: SipHash-1-3 over the string's code points stored at the width
 * PEP 393 gives it — 1, 2 or 4 bytes each, little-endian. */
int64_t lt_hash_str(lt_str *s) {
    if (s->hash != 0) return (int64_t)s->hash;
    if (s->length == 0) return 0;
    int32_t widest = 0;
    for (int64_t i = 0; i < s->size;) {
        int n;
        int32_t c = lt_decode(s->bytes + i, &n);
        if (c > widest) widest = c;
        i += n;
    }
    int width = widest < 0x100 ? 1 : widest < 0x10000 ? 2 : 4;
    uint8_t *units = lt_malloc(lt_bytes(s->length, (size_t)width));
    int64_t k = 0;
    for (int64_t i = 0; i < s->size;) {
        int n;
        int32_t c = lt_decode(s->bytes + i, &n);
        for (int w = 0; w < width; w++) units[k++] = (uint8_t)((uint32_t)c >> (8 * w));
        i += n;
    }
    int64_t h = (int64_t)lt_siphash13(units, s->length * width);
    free(units);
    if (h == -1) h = -2;
    if (lt_count_of(&s->cell) > 0) s->hash = (uint64_t)h;
    return h;
}

static int64_t lt_hash_str_value(const void *value) {
    return lt_hash_str(*(lt_str *const *)value);
}

/* Dicts ------------------------------------------------------------------------------------ */

/* An entry: its hash, whether it is live, then the key and the value at 8-byte offsets. */
#define LT_ROUND8(n) (((n) + 7) & ~(size_t)7)
#define LT_DICT_EMPTY (-1)
#define LT_DICT_DUMMY (-2)

static size_t lt_dict_entry_size(const lt_dict *d) {
    return 16 + LT_ROUND8(d->key->size) + LT_ROUND8(d->value->size);
}

static char *lt_dict_entry(const lt_dict *d, int64_t i) {
    return d->entries + (size_t)i * lt_dict_entry_size(d);
}

#define LT_ENTRY_HASH(e) (*(int64_t *)(e))
#define LT_ENTRY_LIVE(e) (*(int64_t *)((e) + 8))
#define LT_ENTRY_KEY(e) ((e) + 16)
#define LT_ENTRY_VALUE(d, e) ((e) + 16 + LT_ROUND8((d)->key->size))

lt_dict *lt_dict_new(const lt_type *key, const lt_type *value) {
    lt_dict *d = lt_alloc(sizeof(lt_dict));
    d->key = key;
    d->value = value;
    d->len = 0;
    d->used = 0;
    d->cap = 0;
    d->entries = NULL;
    d->mask = 7;
    d->index = lt_malloc(sizeof(int64_t) * 8);
    for (int i = 0; i < 8; i++) d->index[i] = LT_DICT_EMPTY;
    return d;
}

static int64_t lt_key_hash(const lt_type *type, const void *key) {
    return lt_hash_part(type, key);
}

/* The slot of the index table holding `key`'s entry, or the empty slot where it would go. */
static int64_t lt_dict_probe(const lt_dict *d, const void *key, int64_t hash, bool *found) {
    uint64_t perturb = (uint64_t)hash;
    uint64_t i = (uint64_t)hash & (uint64_t)d->mask;
    int64_t free_slot = -1;
    for (;;) {
        int64_t at = d->index[i];
        if (at == LT_DICT_EMPTY) {
            *found = false;
            return free_slot >= 0 ? free_slot : (int64_t)i;
        }
        if (at == LT_DICT_DUMMY) {
            if (free_slot < 0) free_slot = (int64_t)i;
        } else {
            char *e = lt_dict_entry(d, at);
            if (LT_ENTRY_HASH(e) == hash && d->key->eq(LT_ENTRY_KEY(e), key)) {
                *found = true;
                return (int64_t)i;
            }
        }
        perturb >>= 5;
        i = (i * 5 + perturb + 1) & (uint64_t)d->mask;
    }
}

/* Rebuild the index for `size` slots, dropping dead entries but keeping their order. */
static void lt_dict_rebuild(lt_dict *d, int64_t size) {
    size_t entry = lt_dict_entry_size(d);
    int64_t live = 0;
    for (int64_t i = 0; i < d->used; i++) {
        char *e = lt_dict_entry(d, i);
        if (!LT_ENTRY_LIVE(e)) continue;
        if (live != i) memmove(lt_dict_entry(d, live), e, entry);
        live++;
    }
    d->used = live;
    free(d->index);
    d->mask = size - 1;
    d->index = lt_malloc(lt_bytes(size, sizeof(int64_t)));
    for (int64_t i = 0; i < size; i++) d->index[i] = LT_DICT_EMPTY;
    for (int64_t k = 0; k < d->used; k++) {
        char *e = lt_dict_entry(d, k);
        uint64_t perturb = (uint64_t)LT_ENTRY_HASH(e);
        uint64_t i = perturb & (uint64_t)d->mask;
        while (d->index[i] != LT_DICT_EMPTY) {
            perturb >>= 5;
            i = (i * 5 + perturb + 1) & (uint64_t)d->mask;
        }
        d->index[i] = k;
    }
}

void lt_dict_drop(lt_dict *d) {
    if (d == NULL || !lt_dec(d)) return;
    for (int64_t i = 0; i < d->used; i++) {
        char *e = lt_dict_entry(d, i);
        if (!LT_ENTRY_LIVE(e)) continue;
        if (d->key->dec != NULL) d->key->dec(LT_ENTRY_KEY(e));
        if (d->value->dec != NULL) d->value->dec(LT_ENTRY_VALUE(d, e));
    }
    free(d->entries);
    free(d->index);
    lt_free(d);
}

lt_dict *lt_dict_copy(const lt_dict *d) {
    lt_dict *c = lt_dict_new(d->key, d->value);
    for (int64_t i = 0; i < d->used; i++) {
        char *e = lt_dict_entry(d, i);
        if (!LT_ENTRY_LIVE(e)) continue;
        if (d->key->inc != NULL) d->key->inc(LT_ENTRY_KEY(e));
        if (d->value->inc != NULL) d->value->inc(LT_ENTRY_VALUE(d, e));
        lt_dict_set(&c, LT_ENTRY_KEY(e), LT_ENTRY_VALUE(d, e));
    }
    return c;
}

void lt_dict_unique(lt_dict **slot) {
    if (lt_unique(*slot)) return;
    lt_dict *c = lt_dict_copy(*slot);
    lt_dict_drop(*slot);
    *slot = c;
}

/* `d[key] = value`, both taken over: the key a dict already holds stays, as in CPython. */
void lt_dict_set(lt_dict **slot, const void *key, const void *value) {
    lt_dict_unique(slot);
    lt_dict *d = *slot;
    int64_t hash = lt_key_hash(d->key, key);
    bool found;
    int64_t at = lt_dict_probe(d, key, hash, &found);
    if (found) {
        char *e = lt_dict_entry(d, d->index[at]);
        if (d->value->dec != NULL) d->value->dec(LT_ENTRY_VALUE(d, e));
        memcpy(LT_ENTRY_VALUE(d, e), value, d->value->size);
        if (d->key->dec != NULL) d->key->dec((void *)key);
        return;
    }
    if (d->used == d->cap) {
        int64_t cap = d->cap < 8 ? 8 : d->cap * 2;
        char *entries = realloc(d->entries, lt_bytes(cap, lt_dict_entry_size(d)));
        if (entries == NULL) lt_panic((lt_at){NULL, 0, NULL}, "MemoryError", "out of memory");
        d->entries = entries;
        d->cap = cap;
    }
    char *e = lt_dict_entry(d, d->used);
    LT_ENTRY_HASH(e) = hash;
    LT_ENTRY_LIVE(e) = 1;
    memcpy(LT_ENTRY_KEY(e), key, d->key->size);
    memcpy(LT_ENTRY_VALUE(d, e), value, d->value->size);
    d->index[at] = d->used;
    d->used++;
    d->len++;
    if (d->used * 3 >= (d->mask + 1) * 2) {
        int64_t size = d->mask + 1;
        while (size * 2 <= d->len * 3) size *= 2;
        lt_dict_rebuild(d, size * 2);
    }
}

static char *lt_dict_find(const lt_dict *d, const void *key) {
    bool found;
    int64_t at = lt_dict_probe(d, key, lt_key_hash(d->key, key), &found);
    return found ? lt_dict_entry(d, d->index[at]) : NULL;
}

LT_NORETURN static void lt_key_error(const lt_dict *d, const void *key, lt_at at) {
    lt_buf b = LT_BUF;
    d->key->repr(&b, key);
    lt_buf_put(&b, "", 1);
    lt_panic(at, "KeyError", b.data);
}

/* `d[key]`: the value, read; a missing key stops the program. */
const void *lt_dict_get(const lt_dict *d, const void *key, lt_at at) {
    char *e = lt_dict_find(d, key);
    if (e == NULL) lt_key_error(d, key, at);
    return LT_ENTRY_VALUE(d, e);
}

/* `d.get(key, default)`: the value or `default`, read. */
const void *lt_dict_get_or(const lt_dict *d, const void *key, const void *otherwise) {
    char *e = lt_dict_find(d, key);
    return e == NULL ? otherwise : LT_ENTRY_VALUE(d, e);
}

/* The slot of `key`'s value, the dict made unique first: `d[key].append(x)`. */
void *lt_dict_slot(lt_dict **slot, const void *key, lt_at at) {
    lt_dict_unique(slot);
    char *e = lt_dict_find(*slot, key);
    if (e == NULL) lt_key_error(*slot, key, at);
    return LT_ENTRY_VALUE(*slot, e);
}

/* `d.setdefault(key, default)`: the slot of the value, `default` stored first when missing. */
void *lt_dict_setdefault(lt_dict **slot, const void *key, const void *otherwise) {
    lt_dict_unique(slot);
    char *e = lt_dict_find(*slot, key);
    if (e != NULL) {
        if ((*slot)->key->dec != NULL) (*slot)->key->dec((void *)key);
        if ((*slot)->value->dec != NULL) (*slot)->value->dec((void *)otherwise);
        return LT_ENTRY_VALUE(*slot, e);
    }
    lt_dict_set(slot, key, otherwise);
    return LT_ENTRY_VALUE(*slot, lt_dict_find(*slot, key));
}

bool lt_dict_contains(const lt_dict *d, const void *key) {
    return lt_dict_find(d, key) != NULL;
}

bool lt_dict_get_optional(const lt_dict *d, const void *key, void *out) {
    char *e = lt_dict_find(d, key);
    memset(out, 0, d->value->size);
    if (e == NULL) return false;
    memcpy(out, LT_ENTRY_VALUE(d, e), d->value->size);
    if (d->value->inc != NULL) d->value->inc(out);
    return true;
}

/* `d.pop(key)`: the value taken out, its count with it. */
bool lt_dict_pop(lt_dict **slot, const void *key, void *out) {
    memset(out, 0, (*slot)->value->size);
    if (lt_dict_find(*slot, key) == NULL) return false;
    lt_dict_unique(slot);
    lt_dict *d = *slot;
    bool found;
    int64_t at = lt_dict_probe(d, key, lt_key_hash(d->key, key), &found);
    char *e = lt_dict_entry(d, d->index[at]);
    memcpy(out, LT_ENTRY_VALUE(d, e), d->value->size);
    if (d->key->dec != NULL) d->key->dec(LT_ENTRY_KEY(e));
    LT_ENTRY_LIVE(e) = 0;
    d->index[at] = LT_DICT_DUMMY;
    d->len--;
    return true;
}

void lt_dict_clear(lt_dict **slot) {
    const lt_type *key = (*slot)->key, *value = (*slot)->value;
    lt_dict_drop(*slot);
    *slot = lt_dict_new(key, value);
}

lt_list *lt_dict_keys(const lt_dict *d) {
    lt_list *l = lt_list_new(d->key, d->len);
    for (int64_t i = 0; i < d->used; i++) {
        char *e = lt_dict_entry(d, i);
        if (!LT_ENTRY_LIVE(e)) continue;
        if (d->key->inc != NULL) d->key->inc(LT_ENTRY_KEY(e));
        lt_list_push(&l, LT_ENTRY_KEY(e));
    }
    return l;
}

lt_list *lt_dict_values(const lt_dict *d) {
    lt_list *l = lt_list_new(d->value, d->len);
    for (int64_t i = 0; i < d->used; i++) {
        char *e = lt_dict_entry(d, i);
        if (!LT_ENTRY_LIVE(e)) continue;
        if (d->value->inc != NULL) d->value->inc(LT_ENTRY_VALUE(d, e));
        lt_list_push(&l, LT_ENTRY_VALUE(d, e));
    }
    return l;
}

/* `d.items()`: a list of `pair`, a struct holding the key at offset 0 and the value at
 * `value_at`. */
lt_list *lt_dict_items(const lt_dict *d, const lt_type *pair, size_t value_at) {
    lt_list *l = lt_list_new(pair, d->len);
    char *item = calloc(1, pair->size);
    for (int64_t i = 0; i < d->used; i++) {
        char *e = lt_dict_entry(d, i);
        if (!LT_ENTRY_LIVE(e)) continue;
        memcpy(item, LT_ENTRY_KEY(e), d->key->size);
        memcpy(item + value_at, LT_ENTRY_VALUE(d, e), d->value->size);
        pair->inc(item);
        lt_list_push(&l, item);
    }
    free(item);
    return l;
}

/* `dict(pairs)`: the pairs of a list, each a struct as `lt_dict_items` lays it out. */
lt_dict *lt_dict_from_pairs(const lt_type *key, const lt_type *value, const lt_list *pairs, size_t value_at) {
    lt_dict *d = lt_dict_new(key, value);
    for (int64_t i = 0; i < pairs->len; i++) {
        char *item = LT_AT(pairs, i);
        if (key->inc != NULL) key->inc(item);
        if (value->inc != NULL) value->inc(item + value_at);
        lt_dict_set(&d, item, item + value_at);
    }
    return d;
}

static bool lt_eq_dict(const void *a, const void *b) {
    const lt_dict *x = *(lt_dict *const *)a, *y = *(lt_dict *const *)b;
    if (x == y) return true;
    if (x->len != y->len) return false;
    for (int64_t i = 0; i < x->used; i++) {
        char *e = lt_dict_entry(x, i);
        if (!LT_ENTRY_LIVE(e)) continue;
        char *f = lt_dict_find(y, LT_ENTRY_KEY(e));
        if (f == NULL || !x->value->eq(LT_ENTRY_VALUE(x, e), LT_ENTRY_VALUE(y, f))) return false;
    }
    return true;
}

static void lt_repr_dict(lt_buf *b, const void *a) {
    const lt_dict *d = *(lt_dict *const *)a;
    lt_buf_put(b, "{", 1);
    bool first = true;
    for (int64_t i = 0; i < d->used; i++) {
        char *e = lt_dict_entry(d, i);
        if (!LT_ENTRY_LIVE(e)) continue;
        if (!first) lt_buf_put(b, ", ", 2);
        first = false;
        d->key->repr(b, LT_ENTRY_KEY(e));
        lt_buf_put(b, ": ", 2);
        d->value->repr(b, LT_ENTRY_VALUE(d, e));
    }
    lt_buf_put(b, "}", 1);
}

static int lt_cmp_dict(const void *a, const void *b, lt_at at) {
    (void)a;
    (void)b;
    lt_unorderable(at, "dict");
}

static void lt_inc_dict(void *value) {
    if (*(lt_dict **)value != NULL) lt_inc(*(lt_dict **)value);
}

static void lt_dec_dict(void *value) {
    lt_dict_drop(*(lt_dict **)value);
}

static void lt_share_dict(void *value) {
    lt_dict *d = *(lt_dict **)value;
    if (lt_count_of(&d->cell) <= 0) return;
    d->cell.count = -d->cell.count;
    for (int64_t i = 0; i < d->used; i++) {
        char *e = lt_dict_entry(d, i);
        if (!LT_ENTRY_LIVE(e)) continue;
        if (d->key->share != NULL) d->key->share(LT_ENTRY_KEY(e));
        if (d->value->share != NULL) d->value->share(LT_ENTRY_VALUE(d, e));
    }
}

static void lt_show_dict(lt_buf *b, const void *a) {
    const lt_dict *d = *(lt_dict *const *)a;
    lt_buf_put(b, "{", 1);
    bool first = true;
    for (int64_t i = 0; i < d->used; i++) {
        char *e = lt_dict_entry(d, i);
        if (!LT_ENTRY_LIVE(e)) continue;
        if (!first) lt_buf_put(b, ", ", 2);
        first = false;
        d->key->show(b, LT_ENTRY_KEY(e));
        lt_buf_put(b, ": ", 2);
        d->value->show(b, LT_ENTRY_VALUE(d, e));
    }
    lt_buf_put(b, "}", 1);
}

const lt_type lt_type_dict = {sizeof(lt_dict *), lt_inc_dict, lt_dec_dict, lt_eq_dict, lt_cmp_dict, NULL, lt_repr_dict, lt_repr_dict, lt_share_dict, lt_show_dict};

/* Sets, as CPython's setobject.c ------------------------------------------------------------ */

#define LT_LINEAR_PROBES 9
#define LT_PERTURB_SHIFT 5
#define LT_SET_EMPTY 0
#define LT_SET_ACTIVE 1
#define LT_SET_DUMMY 2

/* A slot: its hash, its state, then the element at an 8-byte offset. */
static size_t lt_set_slot_size(const lt_set *s) {
    return 16 + LT_ROUND8(s->type->size);
}

static char *lt_set_slot(const lt_set *s, uint64_t i) {
    return s->table + i * lt_set_slot_size(s);
}

#define LT_SLOT_HASH(e) (*(int64_t *)(e))
#define LT_SLOT_STATE(e) (*(int64_t *)((e) + 8))
#define LT_SLOT_KEY(e) ((e) + 16)

lt_set *lt_set_new(const lt_type *type) {
    lt_set *s = lt_alloc(sizeof(lt_set));
    s->type = type;
    s->fill = 0;
    s->used = 0;
    s->mask = 7;
    s->finger = 0;
    s->table = calloc(8, 16 + LT_ROUND8(type->size));
    return s;
}

/* set_insert_clean: a slot for `hash` in a table with no dummies and no equal element. */
static void lt_set_insert_clean(lt_set *s, const void *key, int64_t hash) {
    uint64_t mask = (uint64_t)s->mask;
    uint64_t perturb = (uint64_t)hash;
    uint64_t i = (uint64_t)hash & mask;
    char *e;
    for (;;) {
        e = lt_set_slot(s, i);
        if (LT_SLOT_STATE(e) == LT_SET_EMPTY) break;
        bool found = false;
        if (i + LT_LINEAR_PROBES <= mask) {
            for (int j = 0; j < LT_LINEAR_PROBES; j++) {
                e = lt_set_slot(s, i + 1 + (uint64_t)j);
                if (LT_SLOT_STATE(e) == LT_SET_EMPTY) {
                    found = true;
                    break;
                }
            }
        }
        if (found) break;
        perturb >>= LT_PERTURB_SHIFT;
        i = (i * 5 + 1 + perturb) & mask;
    }
    LT_SLOT_HASH(e) = hash;
    LT_SLOT_STATE(e) = LT_SET_ACTIVE;
    memcpy(LT_SLOT_KEY(e), key, s->type->size);
}

/* set_table_resize: the smallest power of two above `minused`, the active elements reinserted
 * in table order. */
static void lt_set_resize(lt_set *s, int64_t minused) {
    int64_t size = 8;
    while (size <= minused) size <<= 1;
    char *old = s->table;
    int64_t old_mask = s->mask;
    size_t slot = lt_set_slot_size(s);
    s->table = calloc((size_t)size, slot);
    s->mask = size - 1;
    s->fill = s->used;
    for (int64_t i = 0; i <= old_mask; i++) {
        char *e = old + (size_t)i * slot;
        if (LT_SLOT_STATE(e) == LT_SET_ACTIVE) lt_set_insert_clean(s, LT_SLOT_KEY(e), LT_SLOT_HASH(e));
    }
    free(old);
}

/* set_add_entry: add `key` (taken over) unless an equal element is there, in which case `key`
 * is dropped. */
static void lt_set_add_hashed(lt_set *s, const void *key, int64_t hash) {
    for (;;) {
        uint64_t mask = (uint64_t)s->mask;
        uint64_t i = (uint64_t)hash & mask;
        uint64_t perturb = (uint64_t)hash;
        char *free_slot = NULL;
        for (;;) {
            char *e = lt_set_slot(s, i);
            int probes = (i + LT_LINEAR_PROBES <= mask) ? LT_LINEAR_PROBES : 0;
            bool unused = false;
            do {
                if (LT_SLOT_STATE(e) == LT_SET_EMPTY) {
                    unused = true;
                    break;
                }
                if (LT_SLOT_STATE(e) == LT_SET_ACTIVE && LT_SLOT_HASH(e) == hash && s->type->eq(LT_SLOT_KEY(e), key)) {
                    if (s->type->dec != NULL) s->type->dec((void *)key);
                    return;
                }
                if (LT_SLOT_STATE(e) == LT_SET_DUMMY && free_slot == NULL) free_slot = e;
                e += lt_set_slot_size(s);
            } while (probes--);
            if (unused) {
                if (free_slot != NULL) {
                    LT_SLOT_HASH(free_slot) = hash;
                    LT_SLOT_STATE(free_slot) = LT_SET_ACTIVE;
                    memcpy(LT_SLOT_KEY(free_slot), key, s->type->size);
                    s->used++;
                    return;
                }
                LT_SLOT_HASH(e) = hash;
                LT_SLOT_STATE(e) = LT_SET_ACTIVE;
                memcpy(LT_SLOT_KEY(e), key, s->type->size);
                s->fill++;
                s->used++;
                if ((uint64_t)s->fill * 5 < mask * 3) return;
                lt_set_resize(s, s->used > 50000 ? s->used * 2 : s->used * 4);
                return;
            }
            perturb >>= LT_PERTURB_SHIFT;
            i = (i * 5 + 1 + perturb) & mask;
        }
    }
}

static char *lt_set_find(const lt_set *s, const void *key, int64_t hash) {
    uint64_t mask = (uint64_t)s->mask;
    uint64_t i = (uint64_t)hash & mask;
    uint64_t perturb = (uint64_t)hash;
    for (;;) {
        char *e = lt_set_slot(s, i);
        int probes = (i + LT_LINEAR_PROBES <= mask) ? LT_LINEAR_PROBES : 0;
        do {
            if (LT_SLOT_STATE(e) == LT_SET_EMPTY) return NULL;
            if (LT_SLOT_STATE(e) == LT_SET_ACTIVE && LT_SLOT_HASH(e) == hash && s->type->eq(LT_SLOT_KEY(e), key)) return e;
            e += lt_set_slot_size(s);
        } while (probes--);
        perturb >>= LT_PERTURB_SHIFT;
        i = (i * 5 + 1 + perturb) & mask;
    }
}

void lt_set_drop(lt_set *s) {
    if (s == NULL || !lt_dec(s)) return;
    if (s->type->dec != NULL) {
        for (int64_t i = 0; i <= s->mask; i++) {
            char *e = lt_set_slot(s, (uint64_t)i);
            if (LT_SLOT_STATE(e) == LT_SET_ACTIVE) s->type->dec(LT_SLOT_KEY(e));
        }
    }
    free(s->table);
    lt_free(s);
}

/* set_merge: `other`'s elements, counted once more, added to `s` — one resize first when they
 * may not fit, then slot for slot when `s` is empty with the same table and `other` has no
 * dummies, reinserted in table order when `s` is empty, added one by one otherwise. */
static void lt_set_merge(lt_set *s, const lt_set *other) {
    if (other == s || other->used == 0) return;
    if ((s->fill + other->used) * 5 >= s->mask * 3) lt_set_resize(s, (s->used + other->used) * 2);
    size_t slot = lt_set_slot_size(s);
    if (s->fill == 0 && s->mask == other->mask && other->fill == other->used) {
        memcpy(s->table, other->table, (size_t)(other->mask + 1) * slot);
        s->fill = other->fill;
        s->used = other->used;
        if (s->type->inc != NULL) {
            for (int64_t i = 0; i <= s->mask; i++) {
                char *e = lt_set_slot(s, (uint64_t)i);
                if (LT_SLOT_STATE(e) == LT_SET_ACTIVE) s->type->inc(LT_SLOT_KEY(e));
            }
        }
        return;
    }
    if (s->fill == 0) {
        s->fill = other->used;
        s->used = other->used;
        for (int64_t i = 0; i <= other->mask; i++) {
            char *e = lt_set_slot(other, (uint64_t)i);
            if (LT_SLOT_STATE(e) != LT_SET_ACTIVE) continue;
            if (s->type->inc != NULL) s->type->inc(LT_SLOT_KEY(e));
            lt_set_insert_clean(s, LT_SLOT_KEY(e), LT_SLOT_HASH(e));
        }
        return;
    }
    for (int64_t i = 0; i <= other->mask; i++) {
        char *e = lt_set_slot(other, (uint64_t)i);
        if (LT_SLOT_STATE(e) != LT_SET_ACTIVE) continue;
        if (s->type->inc != NULL) s->type->inc(LT_SLOT_KEY(e));
        lt_set_add_hashed(s, LT_SLOT_KEY(e), LT_SLOT_HASH(e));
    }
}

/* set_copy: a new set with `s` merged in. */
lt_set *lt_set_copy(const lt_set *s) {
    lt_set *c = lt_set_new(s->type);
    lt_set_merge(c, s);
    return c;
}

void lt_set_unique(lt_set **slot) {
    if (lt_unique(*slot)) return;
    lt_set *c = lt_set_copy(*slot);
    lt_set_drop(*slot);
    *slot = c;
}

void lt_set_add(lt_set **slot, const void *key) {
    lt_set_unique(slot);
    lt_set_add_hashed(*slot, key, lt_hash_part((*slot)->type, key));
}

bool lt_set_contains(const lt_set *s, const void *key) {
    return lt_set_find(s, key, lt_hash_part(s->type, key)) != NULL;
}

static bool lt_set_take(lt_set **slot, const void *key) {
    if (!lt_set_contains(*slot, key)) return false;
    lt_set_unique(slot);
    lt_set *s = *slot;
    char *e = lt_set_find(s, key, lt_hash_part(s->type, key));
    if (s->type->dec != NULL) s->type->dec(LT_SLOT_KEY(e));
    LT_SLOT_STATE(e) = LT_SET_DUMMY;
    LT_SLOT_HASH(e) = -1;
    s->used--;
    return true;
}

void lt_set_discard(lt_set **slot, const void *key) {
    lt_set_take(slot, key);
}

void lt_set_remove(lt_set **slot, const void *key, lt_at at) {
    if (lt_set_take(slot, key)) return;
    lt_buf b = LT_BUF;
    (*slot)->type->repr(&b, key);
    lt_buf_put(&b, "", 1);
    lt_panic(at, "KeyError", b.data);
}

/* set_pop: the first active slot from the finger on, taken out. */
bool lt_set_pop(lt_set **slot, void *out) {
    memset(out, 0, (*slot)->type->size);
    if ((*slot)->used == 0) return false;
    lt_set_unique(slot);
    lt_set *s = *slot;
    uint64_t i = (uint64_t)s->finger & (uint64_t)s->mask;
    for (;;) {
        char *e = lt_set_slot(s, i);
        if (LT_SLOT_STATE(e) == LT_SET_ACTIVE) {
            memcpy(out, LT_SLOT_KEY(e), s->type->size);
            LT_SLOT_STATE(e) = LT_SET_DUMMY;
            LT_SLOT_HASH(e) = -1;
            s->used--;
            s->finger = (int64_t)i + 1;
            return true;
        }
        i = i + 1 > (uint64_t)s->mask ? 0 : i + 1;
    }
}

/* The elements of `s` in table order, counted once more. */
lt_list *lt_set_list(const lt_set *s) {
    lt_list *l = lt_list_new(s->type, s->used);
    for (int64_t i = 0; i <= s->mask; i++) {
        char *e = lt_set_slot(s, (uint64_t)i);
        if (LT_SLOT_STATE(e) != LT_SET_ACTIVE) continue;
        if (s->type->inc != NULL) s->type->inc(LT_SLOT_KEY(e));
        lt_list_push(&l, LT_SLOT_KEY(e));
    }
    return l;
}

/* `set(items)`: each element of a list added in order, counted once more. */
lt_set *lt_set_from_list(const lt_type *type, const lt_list *items) {
    lt_set *s = lt_set_new(type);
    for (int64_t i = 0; i < items->len; i++) {
        if (type->inc != NULL) type->inc(LT_AT(items, i));
        lt_set_add_hashed(s, LT_AT(items, i), lt_hash_part(type, LT_AT(items, i)));
    }
    return s;
}

/* A set display of three or more constants as CPython 3.12 and 3.13 build it, which CI and the
 * harness run: the compiler folds the items into a frozenset (`built`, the items added in order),
 * rebuilds that frozenset from its own iteration order when it merges constants, and the code
 * merges it into a new empty set. CPython 3.14 adds the items in order. Takes `built` over. */
lt_set *lt_set_folded(lt_set *built) {
    lt_set *again = lt_set_new(built->type);
    for (int64_t i = 0; i <= built->mask; i++) {
        char *e = lt_set_slot(built, (uint64_t)i);
        if (LT_SLOT_STATE(e) != LT_SET_ACTIVE) continue;
        if (built->type->inc != NULL) built->type->inc(LT_SLOT_KEY(e));
        lt_set_add_hashed(again, LT_SLOT_KEY(e), LT_SLOT_HASH(e));
    }
    lt_set *result = lt_set_new(built->type);
    lt_set_merge(result, again);
    lt_set_drop(again);
    lt_set_drop(built);
    return result;
}

/* `a | b`: a copy of `a` with `b` merged in. */
lt_set *lt_set_union(const lt_set *a, const lt_set *b) {
    lt_set *c = lt_set_copy(a);
    lt_set_merge(c, b);
    return c;
}

/* `a & b`: `b` walked in table order unless it is the larger, each element the other holds
 * added. */
lt_set *lt_set_intersection(const lt_set *a, const lt_set *b) {
    const lt_set *small = b, *large = a;
    if (b->used > a->used) {
        small = a;
        large = b;
    }
    lt_set *c = lt_set_new(a->type);
    for (int64_t i = 0; i <= small->mask; i++) {
        char *e = lt_set_slot(small, (uint64_t)i);
        if (LT_SLOT_STATE(e) != LT_SET_ACTIVE) continue;
        if (lt_set_find(large, LT_SLOT_KEY(e), LT_SLOT_HASH(e)) == NULL) continue;
        if (a->type->inc != NULL) a->type->inc(LT_SLOT_KEY(e));
        lt_set_add_hashed(c, LT_SLOT_KEY(e), LT_SLOT_HASH(e));
    }
    return c;
}

/* `a - b`: `a` copied then `b`'s elements discarded when `a` is over four times larger, else
 * `a` walked in table order, each element `b` lacks added. */
lt_set *lt_set_difference(const lt_set *a, const lt_set *b) {
    if ((a->used >> 2) > b->used) {
        lt_set *c = lt_set_copy(a);
        for (int64_t i = 0; i <= b->mask; i++) {
            char *e = lt_set_slot(b, (uint64_t)i);
            if (LT_SLOT_STATE(e) == LT_SET_ACTIVE) lt_set_take(&c, LT_SLOT_KEY(e));
        }
        return c;
    }
    lt_set *c = lt_set_new(a->type);
    for (int64_t i = 0; i <= a->mask; i++) {
        char *e = lt_set_slot(a, (uint64_t)i);
        if (LT_SLOT_STATE(e) != LT_SET_ACTIVE) continue;
        if (lt_set_find(b, LT_SLOT_KEY(e), LT_SLOT_HASH(e)) != NULL) continue;
        if (a->type->inc != NULL) a->type->inc(LT_SLOT_KEY(e));
        lt_set_add_hashed(c, LT_SLOT_KEY(e), LT_SLOT_HASH(e));
    }
    return c;
}

bool lt_set_issubset(const lt_set *a, const lt_set *b) {
    if (a->used > b->used) return false;
    for (int64_t i = 0; i <= a->mask; i++) {
        char *e = lt_set_slot(a, (uint64_t)i);
        if (LT_SLOT_STATE(e) == LT_SET_ACTIVE && lt_set_find(b, LT_SLOT_KEY(e), LT_SLOT_HASH(e)) == NULL) return false;
    }
    return true;
}

static bool lt_eq_set(const void *a, const void *b) {
    const lt_set *x = *(lt_set *const *)a, *y = *(lt_set *const *)b;
    return x->used == y->used && lt_set_issubset(x, y);
}

static void lt_repr_set(lt_buf *b, const void *a) {
    const lt_set *s = *(lt_set *const *)a;
    if (s->used == 0) {
        lt_buf_puts(b, "set()");
        return;
    }
    lt_buf_put(b, "{", 1);
    bool first = true;
    for (int64_t i = 0; i <= s->mask; i++) {
        char *e = lt_set_slot(s, (uint64_t)i);
        if (LT_SLOT_STATE(e) != LT_SET_ACTIVE) continue;
        if (!first) lt_buf_put(b, ", ", 2);
        first = false;
        s->type->repr(b, LT_SLOT_KEY(e));
    }
    lt_buf_put(b, "}", 1);
}

static int lt_cmp_set(const void *a, const void *b, lt_at at) {
    (void)a;
    (void)b;
    lt_unorderable(at, "set");
}

static void lt_inc_set(void *value) {
    if (*(lt_set **)value != NULL) lt_inc(*(lt_set **)value);
}

static void lt_dec_set(void *value) {
    lt_set_drop(*(lt_set **)value);
}

static void lt_share_set(void *value) {
    lt_set *s = *(lt_set **)value;
    if (lt_count_of(&s->cell) <= 0) return;
    s->cell.count = -s->cell.count;
    if (s->type->share == NULL) return;
    for (int64_t i = 0; i <= s->mask; i++) {
        char *e = lt_set_slot(s, (uint64_t)i);
        if (LT_SLOT_STATE(e) == LT_SET_ACTIVE) s->type->share(LT_SLOT_KEY(e));
    }
}

static int lt_cmp_text(const void *a, const void *b) {
    return strcmp(*(char *const *)a, *(char *const *)b);
}

/* A set's elements shown and sorted as text, as the Python target's `show` sorts them. */
static void lt_show_set(lt_buf *b, const void *a) {
    const lt_set *s = *(lt_set *const *)a;
    if (s->used == 0) {
        lt_buf_puts(b, "set()");
        return;
    }
    char **texts = lt_malloc(lt_bytes(s->used, sizeof(char *)));
    int64_t n = 0;
    for (int64_t i = 0; i <= s->mask; i++) {
        char *e = lt_set_slot(s, (uint64_t)i);
        if (LT_SLOT_STATE(e) != LT_SET_ACTIVE) continue;
        lt_buf t = LT_BUF;
        s->type->show(&t, LT_SLOT_KEY(e));
        lt_buf_put(&t, "", 1);
        texts[n++] = t.data;
    }
    qsort(texts, (size_t)n, sizeof(char *), lt_cmp_text);
    lt_buf_put(b, "{", 1);
    for (int64_t i = 0; i < n; i++) {
        if (i > 0) lt_buf_put(b, ", ", 2);
        lt_buf_puts(b, texts[i]);
        free(texts[i]);
    }
    free(texts);
    lt_buf_put(b, "}", 1);
}

const lt_type lt_type_set = {sizeof(lt_set *), lt_inc_set, lt_dec_set, lt_eq_set, lt_cmp_set, NULL, lt_repr_set, lt_repr_set, lt_share_set, lt_show_set};

/* Strings: UTF-8 text by code point, CPython's repr and its format mini-language. Included by
 * lotml.c. Character classes and case mapping cover ASCII, Latin-1, Latin Extended-A, Greek and
 * Cyrillic; other scripts are letters without case. */

/* UTF-8 ------------------------------------------------------------------------------------ */

static int32_t lt_decode(const char *text, int *length) {
    const unsigned char *p = (const unsigned char *)text;
    if (p[0] < 0x80) {
        *length = 1;
        return p[0];
    }
    if ((p[0] & 0xE0) == 0xC0) {
        *length = 2;
        return ((p[0] & 0x1F) << 6) | (p[1] & 0x3F);
    }
    if ((p[0] & 0xF0) == 0xE0) {
        *length = 3;
        return ((p[0] & 0x0F) << 12) | ((p[1] & 0x3F) << 6) | (p[2] & 0x3F);
    }
    *length = 4;
    return ((p[0] & 0x07) << 18) | ((p[1] & 0x3F) << 12) | ((p[2] & 0x3F) << 6) | (p[3] & 0x3F);
}

static int lt_encode(int32_t code, char out[4]) {
    if (code < 0x80) {
        out[0] = (char)code;
        return 1;
    }
    if (code < 0x800) {
        out[0] = (char)(0xC0 | (code >> 6));
        out[1] = (char)(0x80 | (code & 0x3F));
        return 2;
    }
    if (code < 0x10000) {
        out[0] = (char)(0xE0 | (code >> 12));
        out[1] = (char)(0x80 | ((code >> 6) & 0x3F));
        out[2] = (char)(0x80 | (code & 0x3F));
        return 3;
    }
    out[0] = (char)(0xF0 | (code >> 18));
    out[1] = (char)(0x80 | ((code >> 12) & 0x3F));
    out[2] = (char)(0x80 | ((code >> 6) & 0x3F));
    out[3] = (char)(0x80 | (code & 0x3F));
    return 4;
}

static void lt_buf_code(lt_buf *b, int32_t code) {
    char bytes[4];
    lt_buf_put(b, bytes, (size_t)lt_encode(code, bytes));
}

static int64_t lt_count_points(const char *bytes, int64_t size) {
    int64_t n = 0;
    for (int64_t i = 0; i < size; i++) {
        if (((unsigned char)bytes[i] & 0xC0) != 0x80) n++;
    }
    return n;
}

/* The byte offset of code point `index` of `s`, 0 <= index <= length. */
static int64_t lt_offset(const lt_str *s, int64_t index) {
    if (s->size == s->length) return index;
    int64_t at = 0;
    for (int64_t i = 0; i < index; i++) {
        int n;
        lt_decode(s->bytes + at, &n);
        at += n;
    }
    return at;
}

/* Code point classes, as Python's str methods see them -------------------------------------- */

static bool lt_is_space(int32_t c) {
    return (c >= 0x09 && c <= 0x0D) || (c >= 0x1C && c <= 0x20) || c == 0x85 || c == 0xA0 || c == 0x1680 ||
           (c >= 0x2000 && c <= 0x200A) || c == 0x2028 || c == 0x2029 || c == 0x202F || c == 0x205F || c == 0x3000;
}

static bool lt_is_digit(int32_t c) {
    return (c >= '0' && c <= '9') || c == 0xB2 || c == 0xB3 || c == 0xB9 || (c >= 0x660 && c <= 0x669) ||
           (c >= 0x6F0 && c <= 0x6F9) || (c >= 0x966 && c <= 0x96F) || (c >= 0x9E6 && c <= 0x9EF) || c == 0x2070 ||
           (c >= 0x2074 && c <= 0x2079) || (c >= 0x2080 && c <= 0x2089) || (c >= 0x2460 && c <= 0x2468) ||
           (c >= 0xFF10 && c <= 0xFF19);
}

/* The uppercase of `c`, a single code point; `c` when it has none. */
static int32_t lt_upper_point(int32_t c) {
    if (c >= 'a' && c <= 'z') return c - 32;
    if (c < 0x80) return c;
    if (c == 0xB5) return 0x39C;
    if (c >= 0xE0 && c <= 0xFE && c != 0xF7) return c - 32;
    if (c == 0xFF) return 0x178;
    if (c >= 0x100 && c <= 0x17F) {
        if (c == 0x131) return 'I';
        if (c == 0x17F) return 'S';
        if ((c >= 0x139 && c <= 0x148) || (c >= 0x179 && c <= 0x17E)) return (c % 2 == 0) ? c - 1 : c;
        if (c == 0x138 || c == 0x149 || c == 0x178) return c;
        return (c % 2 == 1) ? c - 1 : c;
    }
    if (c >= 0x3B1 && c <= 0x3C9) return c == 0x3C2 ? 0x3A3 : c - 32;
    if (c == 0x3AC) return 0x386;
    if (c >= 0x3AD && c <= 0x3AF) return c - 37;
    if (c == 0x3CC) return 0x38C;
    if (c == 0x3CD || c == 0x3CE) return c - 63;
    if (c >= 0x430 && c <= 0x44F) return c - 32;
    if (c >= 0x450 && c <= 0x45F) return c - 80;
    if ((c >= 0x460 && c <= 0x481) || (c >= 0x48A && c <= 0x4BF)) return (c % 2 == 1) ? c - 1 : c;
    return c;
}

static int32_t lt_lower_point(int32_t c) {
    if (c >= 'A' && c <= 'Z') return c + 32;
    if (c < 0x80) return c;
    if (c >= 0xC0 && c <= 0xDE && c != 0xD7) return c + 32;
    if (c == 0x178) return 0xFF;
    if (c >= 0x100 && c <= 0x17F) {
        if (c == 0x130) return 'i';
        if ((c >= 0x139 && c <= 0x148) || (c >= 0x179 && c <= 0x17E)) return (c % 2 == 1) ? c + 1 : c;
        if (c == 0x138 || c == 0x149 || c == 0x131 || c == 0x17F) return c;
        return (c % 2 == 0) ? c + 1 : c;
    }
    if (c >= 0x391 && c <= 0x3A9 && c != 0x3A2) return c + 32;
    if (c == 0x386) return 0x3AC;
    if (c >= 0x388 && c <= 0x38A) return c + 37;
    if (c == 0x38C) return 0x3CC;
    if (c == 0x38E || c == 0x38F) return c + 63;
    if (c >= 0x410 && c <= 0x42F) return c + 32;
    if (c >= 0x400 && c <= 0x40F) return c + 80;
    if ((c >= 0x460 && c <= 0x481) || (c >= 0x48A && c <= 0x4BF)) return (c % 2 == 0) ? c + 1 : c;
    return c;
}

static bool lt_is_upper_point(int32_t c) {
    return lt_lower_point(c) != c;
}

static bool lt_is_lower_point(int32_t c) {
    return lt_upper_point(c) != c || c == 0xDF || c == 0x138 || c == 0x149;
}

static bool lt_is_cased(int32_t c) {
    return lt_is_upper_point(c) || lt_is_lower_point(c);
}

static bool lt_is_alpha(int32_t c) {
    if (c < 0x80) return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z');
    if (lt_is_cased(c)) return true;
    return c == 0xAA || c == 0xBA || (c >= 0xC0 && c <= 0x24F && c != 0xD7 && c != 0xF7) || (c >= 0x370 && c <= 0x3FF && c != 0x37E && c != 0x387) ||
           (c >= 0x400 && c <= 0x481) || (c >= 0x48A && c <= 0x52F) || (c >= 0x5D0 && c <= 0x5EA) ||
           (c >= 0x620 && c <= 0x64A) || (c >= 0x3041 && c <= 0x3096) || (c >= 0x30A1 && c <= 0x30FA) ||
           (c >= 0x4E00 && c <= 0x9FFF) || (c >= 0xAC00 && c <= 0xD7A3);
}

/* Python's str.isprintable, for repr: control, format and separator characters are escaped. */
static bool lt_is_printable(int32_t c) {
    if (c < 0x20 || (c >= 0x7F && c <= 0xA0) || c == 0xAD) return false;
    if ((c >= 0x2000 && c <= 0x200F) || (c >= 0x2028 && c <= 0x202F) || (c >= 0x205F && c <= 0x206F)) return false;
    if (c == 0x1680 || c == 0x3000 || c == 0xFEFF || (c >= 0xD800 && c <= 0xF8FF) || (c >= 0xFFF0 && c <= 0xFFFF)) return false;
    return c <= 0x10FFFF;
}

/* Cells ------------------------------------------------------------------------------------ */

static lt_str *lt_str_alloc(int64_t size, int64_t length) {
    lt_str *s = lt_alloc(sizeof(lt_str) + (size_t)size + 1);
    s->size = size;
    s->length = length;
    s->hash = 0;
    s->bytes[size] = '\0';
    return s;
}

lt_str *lt_str_new(const char *bytes, int64_t size) {
    lt_str *s = lt_str_alloc(size, lt_count_points(bytes, size));
    memcpy(s->bytes, bytes, (size_t)size);
    return s;
}

lt_str *lt_str_from_buf(lt_buf *b) {
    lt_str *s = lt_str_new(b->data == NULL ? "" : b->data, (int64_t)b->len);
    lt_buf_free(b);
    return s;
}

void lt_str_drop(lt_str *s) {
    if (s != NULL && lt_dec(s)) lt_free(s);
}

void lt_buf_str(lt_buf *b, const lt_str *s) {
    lt_buf_put(b, s->bytes, (size_t)s->size);
}

/* `str(value)` of the values that are not strings. */
lt_str *lt_str_of_i64(int64_t value) {
    lt_buf b = LT_BUF;
    lt_buf_i64(&b, value);
    return lt_str_from_buf(&b);
}

lt_str *lt_str_of_u64(uint64_t value) {
    lt_buf b = LT_BUF;
    lt_buf_u64(&b, value);
    return lt_str_from_buf(&b);
}

lt_str *lt_str_of_f64(double value) {
    lt_buf b = LT_BUF;
    lt_buf_f64(&b, value);
    return lt_str_from_buf(&b);
}

lt_str *lt_str_of_bool(bool value) {
    return lt_str_new(value ? "True" : "False", value ? 4 : 5);
}

lt_str *lt_str_none(void) {
    return lt_str_new("None", 4);
}

/* `repr(s)`: single quotes unless the text holds a single quote and no double one. */
void lt_buf_str_repr(lt_buf *b, const lt_str *s) {
    bool single = memchr(s->bytes, '\'', (size_t)s->size) != NULL;
    bool dbl = memchr(s->bytes, '"', (size_t)s->size) != NULL;
    char quote = (single && !dbl) ? '"' : '\'';
    lt_buf_put(b, &quote, 1);
    for (int64_t i = 0; i < s->size;) {
        int n;
        int32_t c = lt_decode(s->bytes + i, &n);
        char esc[12];
        if (c == quote || c == '\\') {
            esc[0] = '\\';
            esc[1] = (char)c;
            lt_buf_put(b, esc, 2);
        } else if (c == '\t') {
            lt_buf_put(b, "\\t", 2);
        } else if (c == '\n') {
            lt_buf_put(b, "\\n", 2);
        } else if (c == '\r') {
            lt_buf_put(b, "\\r", 2);
        } else if (!lt_is_printable(c)) {
            int k = c < 0x100 ? snprintf(esc, sizeof esc, "\\x%02x", (unsigned)c)
                    : c < 0x10000 ? snprintf(esc, sizeof esc, "\\u%04x", (unsigned)c)
                                  : snprintf(esc, sizeof esc, "\\U%08x", (unsigned)c);
            lt_buf_put(b, esc, (size_t)k);
        } else {
            lt_buf_put(b, s->bytes + i, (size_t)n);
        }
        i += n;
    }
    lt_buf_put(b, &quote, 1);
}

/* Operators -------------------------------------------------------------------------------- */

lt_str *lt_str_concat(const lt_str *a, const lt_str *b) {
    lt_str *s = lt_str_alloc(a->size + b->size, a->length + b->length);
    memcpy(s->bytes, a->bytes, (size_t)a->size);
    memcpy(s->bytes + a->size, b->bytes, (size_t)b->size);
    return s;
}

lt_str *lt_str_repeat(const lt_str *s, int64_t times, lt_at at) {
    if (times <= 0 || s->size == 0) return lt_str_new("", 0);
    if (s->size > INT64_MAX / times) lt_panic(at, "MemoryError", "the repeated string is too long");
    lt_str *r = lt_str_alloc(s->size * times, s->length * times);
    for (int64_t i = 0; i < times; i++) memcpy(r->bytes + i * s->size, s->bytes, (size_t)s->size);
    return r;
}

int lt_str_compare(const lt_str *a, const lt_str *b) {
    int64_t n = a->size < b->size ? a->size : b->size;
    int c = memcmp(a->bytes, b->bytes, (size_t)n);
    if (c != 0) return c < 0 ? -1 : 1;
    return a->size < b->size ? -1 : a->size > b->size ? 1 : 0;
}

bool lt_str_eq(const lt_str *a, const lt_str *b) {
    return a == b || (a->size == b->size && memcmp(a->bytes, b->bytes, (size_t)a->size) == 0);
}

/* The byte offset of `needle` in `haystack` at or after `from`, or -1. */
static int64_t lt_find_bytes(const lt_str *haystack, const lt_str *needle, int64_t from) {
    if (needle->size == 0) return from <= haystack->size ? from : -1;
    for (int64_t i = from; i + needle->size <= haystack->size; i++) {
        const char *hit = memchr(haystack->bytes + i, needle->bytes[0], (size_t)(haystack->size - needle->size - i + 1));
        if (hit == NULL) return -1;
        i = hit - haystack->bytes;
        if (memcmp(hit, needle->bytes, (size_t)needle->size) == 0) return i;
    }
    return -1;
}

bool lt_str_contains(const lt_str *haystack, const lt_str *needle) {
    return lt_find_bytes(haystack, needle, 0) >= 0;
}

lt_str *lt_str_index(const lt_str *s, int64_t index, lt_at at) {
    if (index < 0) index += s->length;
    if (index < 0 || index >= s->length) lt_panic(at, "IndexError", "string index out of range");
    int64_t from = lt_offset(s, index);
    int n = 1;
    if (s->size != s->length) lt_decode(s->bytes + from, &n);
    return lt_str_new(s->bytes + from, n);
}

/* Python's slice indices: the first index, and how many elements, of `s[lo:hi:step]` over a
 * sequence of `length`. */
void lt_slice_indices(int64_t length, bool has_lo, int64_t lo, bool has_hi, int64_t hi, bool has_step, int64_t step,
                      int64_t *start, int64_t *count, int64_t *by, lt_at at) {
    if (!has_step) step = 1;
    if (step == 0) lt_value_error(at, "slice step cannot be zero");
    if (step > 0) {
        if (!has_lo) lo = 0;
        else if (lo < 0) lo = lo + length < 0 ? 0 : lo + length;
        else if (lo > length) lo = length;
        if (!has_hi) hi = length;
        else if (hi < 0) hi = hi + length < 0 ? 0 : hi + length;
        else if (hi > length) hi = length;
        *count = hi > lo ? (hi - lo - 1) / step + 1 : 0;
    } else {
        if (!has_lo) lo = length - 1;
        else if (lo < 0) lo = lo + length < 0 ? -1 : lo + length;
        else if (lo >= length) lo = length - 1;
        if (!has_hi) hi = -1;
        else if (hi < 0) hi = hi + length < 0 ? -1 : hi + length;
        else if (hi >= length) hi = length - 1;
        *count = lo > hi ? (lo - hi - 1) / (-step) + 1 : 0;
    }
    *start = lo;
    *by = step;
}

lt_str *lt_str_slice(const lt_str *s, bool has_lo, int64_t lo, bool has_hi, int64_t hi, bool has_step, int64_t step, lt_at at) {
    int64_t start, count, by;
    lt_slice_indices(s->length, has_lo, lo, has_hi, hi, has_step, step, &start, &count, &by, at);
    if (count <= 0) return lt_str_new("", 0);
    if (s->size == s->length) {
        lt_str *r = lt_str_alloc(count, count);
        for (int64_t i = 0; i < count; i++) r->bytes[i] = s->bytes[start + i * by];
        return r;
    }
    int64_t *offsets = malloc(sizeof(int64_t) * (size_t)(s->length + 1));
    int64_t at_byte = 0;
    for (int64_t i = 0; i < s->length; i++) {
        offsets[i] = at_byte;
        int n;
        lt_decode(s->bytes + at_byte, &n);
        at_byte += n;
    }
    offsets[s->length] = at_byte;
    lt_buf b = LT_BUF;
    for (int64_t i = 0; i < count; i++) {
        int64_t k = start + i * by;
        lt_buf_put(&b, s->bytes + offsets[k], (size_t)(offsets[k + 1] - offsets[k]));
    }
    free(offsets);
    return lt_str_from_buf(&b);
}

int64_t lt_str_ord(const lt_str *s, lt_at at) {
    if (s->length != 1) {
        lt_panicf(at, "TypeError", "ord() expected a character, but string of length %lld found", (long long)s->length);
    }
    int n;
    return lt_decode(s->bytes, &n);
}

lt_str *lt_str_chr(int64_t code, lt_at at) {
    if (code < 0 || code > 0x10FFFF) lt_value_error(at, "chr() arg not in range(0x110000)");
    char bytes[4];
    return lt_str_new(bytes, lt_encode((int32_t)code, bytes));
}

/* Case ------------------------------------------------------------------------------------- */

static void lt_put_upper(lt_buf *b, int32_t c) {
    if (c == 0xDF) {
        lt_buf_put(b, "SS", 2);
    } else {
        lt_buf_code(b, lt_upper_point(c));
    }
}

static void lt_put_title(lt_buf *b, int32_t c) {
    if (c == 0xDF) {
        lt_buf_put(b, "Ss", 2);
    } else {
        lt_buf_code(b, lt_upper_point(c));
    }
}

static void lt_put_lower(lt_buf *b, int32_t c) {
    if (c == 0x130) {
        lt_buf_put(b, "i\xCC\x87", 3);
    } else {
        lt_buf_code(b, lt_lower_point(c));
    }
}

lt_str *lt_str_lower(const lt_str *s) {
    lt_buf b = LT_BUF;
    for (int64_t i = 0; i < s->size;) {
        int n;
        lt_put_lower(&b, lt_decode(s->bytes + i, &n));
        i += n;
    }
    return lt_str_from_buf(&b);
}

lt_str *lt_str_upper(const lt_str *s) {
    lt_buf b = LT_BUF;
    for (int64_t i = 0; i < s->size;) {
        int n;
        lt_put_upper(&b, lt_decode(s->bytes + i, &n));
        i += n;
    }
    return lt_str_from_buf(&b);
}

lt_str *lt_str_swapcase(const lt_str *s) {
    lt_buf b = LT_BUF;
    for (int64_t i = 0; i < s->size;) {
        int n;
        int32_t c = lt_decode(s->bytes + i, &n);
        if (lt_is_upper_point(c)) {
            lt_put_lower(&b, c);
        } else if (lt_is_lower_point(c)) {
            lt_put_upper(&b, c);
        } else {
            lt_buf_code(&b, c);
        }
        i += n;
    }
    return lt_str_from_buf(&b);
}

/* `title`: a cased character after an uncased one starts a word and is titlecased; the rest of
 * the word is lowercased. */
lt_str *lt_str_title(const lt_str *s) {
    lt_buf b = LT_BUF;
    bool in_word = false;
    for (int64_t i = 0; i < s->size;) {
        int n;
        int32_t c = lt_decode(s->bytes + i, &n);
        if (in_word) {
            lt_put_lower(&b, c);
        } else {
            lt_put_title(&b, c);
        }
        in_word = lt_is_cased(c);
        i += n;
    }
    return lt_str_from_buf(&b);
}

lt_str *lt_str_capitalize(const lt_str *s) {
    lt_buf b = LT_BUF;
    for (int64_t i = 0; i < s->size;) {
        int n;
        int32_t c = lt_decode(s->bytes + i, &n);
        if (i == 0) {
            lt_put_title(&b, c);
        } else {
            lt_put_lower(&b, c);
        }
        i += n;
    }
    return lt_str_from_buf(&b);
}

/* Methods ---------------------------------------------------------------------------------- */

static bool lt_in_chars(int32_t c, const lt_str *chars) {
    if (chars == NULL) return lt_is_space(c);
    for (int64_t i = 0; i < chars->size;) {
        int n;
        if (lt_decode(chars->bytes + i, &n) == c) return true;
        i += n;
    }
    return false;
}

lt_str *lt_str_strip(const lt_str *s, const lt_str *chars, bool left, bool right) {
    int64_t from = 0;
    int64_t to = s->size;
    if (left) {
        while (from < to) {
            int n;
            int32_t c = lt_decode(s->bytes + from, &n);
            if (!lt_in_chars(c, chars)) break;
            from += n;
        }
    }
    if (right) {
        while (to > from) {
            int64_t start = to - 1;
            while (start > from && ((unsigned char)s->bytes[start] & 0xC0) == 0x80) start--;
            int n;
            int32_t c = lt_decode(s->bytes + start, &n);
            if (!lt_in_chars(c, chars)) break;
            to = start;
        }
    }
    return lt_str_new(s->bytes + from, to - from);
}

bool lt_str_startswith(const lt_str *s, const lt_str *prefix) {
    return prefix->size <= s->size && memcmp(s->bytes, prefix->bytes, (size_t)prefix->size) == 0;
}

bool lt_str_endswith(const lt_str *s, const lt_str *suffix) {
    return suffix->size <= s->size && memcmp(s->bytes + s->size - suffix->size, suffix->bytes, (size_t)suffix->size) == 0;
}

lt_str *lt_str_replace(const lt_str *s, const lt_str *old, const lt_str *with, int64_t count) {
    lt_buf b = LT_BUF;
    if (old->size == 0) {
        int64_t done = 0;
        for (int64_t i = 0; i <= s->size;) {
            if (count < 0 || done < count) {
                lt_buf_str(&b, with);
                done++;
            }
            if (i == s->size) break;
            int n;
            lt_decode(s->bytes + i, &n);
            lt_buf_put(&b, s->bytes + i, (size_t)n);
            i += n;
        }
        return lt_str_from_buf(&b);
    }
    int64_t from = 0;
    int64_t done = 0;
    while (count < 0 || done < count) {
        int64_t hit = lt_find_bytes(s, old, from);
        if (hit < 0) break;
        lt_buf_put(&b, s->bytes + from, (size_t)(hit - from));
        lt_buf_str(&b, with);
        from = hit + old->size;
        done++;
    }
    lt_buf_put(&b, s->bytes + from, (size_t)(s->size - from));
    return lt_str_from_buf(&b);
}

int64_t lt_str_count(const lt_str *s, const lt_str *sub) {
    if (sub->size == 0) return s->length + 1;
    int64_t n = 0;
    for (int64_t from = 0;;) {
        int64_t hit = lt_find_bytes(s, sub, from);
        if (hit < 0) return n;
        n++;
        from = hit + sub->size;
    }
}

static bool lt_str_all(const lt_str *s, bool (*test)(int32_t)) {
    if (s->size == 0) return false;
    for (int64_t i = 0; i < s->size;) {
        int n;
        if (!test(lt_decode(s->bytes + i, &n))) return false;
        i += n;
    }
    return true;
}

static bool lt_is_alnum(int32_t c) {
    return lt_is_alpha(c) || lt_is_digit(c);
}

bool lt_str_isalpha(const lt_str *s) {
    return lt_str_all(s, lt_is_alpha);
}

bool lt_str_isdigit(const lt_str *s) {
    return lt_str_all(s, lt_is_digit);
}

bool lt_str_isspace(const lt_str *s) {
    return lt_str_all(s, lt_is_space);
}

bool lt_str_isalnum(const lt_str *s) {
    return lt_str_all(s, lt_is_alnum);
}

/* `isupper`/`islower`: at least one cased character, and every cased one in that case. */
static bool lt_str_cased(const lt_str *s, bool upper) {
    bool seen = false;
    for (int64_t i = 0; i < s->size;) {
        int n;
        int32_t c = lt_decode(s->bytes + i, &n);
        if (upper ? lt_is_lower_point(c) : lt_is_upper_point(c)) return false;
        if (lt_is_cased(c)) seen = true;
        i += n;
    }
    return seen;
}

bool lt_str_isupper(const lt_str *s) {
    return lt_str_cased(s, true);
}

bool lt_str_islower(const lt_str *s) {
    return lt_str_cased(s, false);
}

lt_str *lt_str_zfill(const lt_str *s, int64_t width) {
    if (s->length >= width) return lt_str_new(s->bytes, s->size);
    lt_buf b = LT_BUF;
    int64_t rest = 0;
    if (s->size > 0 && (s->bytes[0] == '+' || s->bytes[0] == '-')) {
        lt_buf_put(&b, s->bytes, 1);
        rest = 1;
    }
    for (int64_t i = s->length; i < width; i++) lt_buf_put(&b, "0", 1);
    lt_buf_put(&b, s->bytes + rest, (size_t)(s->size - rest));
    return lt_str_from_buf(&b);
}

/* `ljust`, `rjust` and `center` (`align` '<', '>' and '^'), as str methods lay them out. */
lt_str *lt_str_pad(const lt_str *s, int64_t width, const lt_str *fill, char align, lt_at at) {
    int32_t code = ' ';
    if (fill != NULL) {
        if (fill->length != 1) lt_panic(at, "TypeError", "The fill character must be exactly one character long");
        int n;
        code = lt_decode(fill->bytes, &n);
    }
    if (s->length >= width) return lt_str_new(s->bytes, s->size);
    int64_t margin = width - s->length;
    int64_t left = align == '<' ? 0 : align == '>' ? margin : margin / 2 + (margin & width & 1);
    lt_buf b = LT_BUF;
    for (int64_t i = 0; i < left; i++) lt_buf_code(&b, code);
    lt_buf_str(&b, s);
    for (int64_t i = left; i < margin; i++) lt_buf_code(&b, code);
    return lt_str_from_buf(&b);
}

/* The format mini-language --------------------------------------------------------------- */

typedef struct lt_spec {
    int32_t fill;
    char align;
    char sign;
    bool no_negative_zero;
    bool alternate;
    int64_t width;
    char grouping;
    int64_t precision;
    char type;
} lt_spec;

static LT_NORETURN void lt_bad_spec(const char *spec, size_t size, const char *kind, lt_at at) {
    lt_panicf(at, "ValueError", "Invalid format specifier '%.*s' for object of type '%s'", (int)size, spec, kind);
}

static bool lt_is_align(char c) {
    return c == '<' || c == '>' || c == '^' || c == '=';
}

/* Parse `[[fill]align][sign][z][#][0][width][grouping][.precision][type]`. */
static lt_spec lt_parse_spec(const char *spec, size_t size, const char *kind, lt_at at) {
    lt_spec s = {' ', 0, 0, false, false, -1, 0, -1, 0};
    size_t i = 0;
    int n = 1;
    int32_t first = size > 0 ? lt_decode(spec, &n) : 0;
    if (size > (size_t)n && lt_is_align(spec[n])) {
        s.fill = first;
        s.align = spec[n];
        i = (size_t)n + 1;
    } else if (size > 0 && lt_is_align(spec[0])) {
        s.align = spec[0];
        i = 1;
    }
    if (i < size && (spec[i] == '+' || spec[i] == '-' || spec[i] == ' ')) s.sign = spec[i++];
    if (i < size && spec[i] == 'z') {
        s.no_negative_zero = true;
        i++;
    }
    if (i < size && spec[i] == '#') {
        s.alternate = true;
        i++;
    }
    if (i < size && spec[i] == '0') {
        if (s.align == 0) {
            s.fill = '0';
            s.align = '=';
        }
        i++;
    }
    if (i < size && spec[i] >= '0' && spec[i] <= '9') {
        s.width = 0;
        while (i < size && spec[i] >= '0' && spec[i] <= '9') s.width = s.width * 10 + (spec[i++] - '0');
    }
    if (i < size && (spec[i] == ',' || spec[i] == '_')) s.grouping = spec[i++];
    if (i < size && spec[i] == '.') {
        i++;
        if (i >= size || spec[i] < '0' || spec[i] > '9') lt_panic(at, "ValueError", "Format specifier missing precision");
        s.precision = 0;
        while (i < size && spec[i] >= '0' && spec[i] <= '9') s.precision = s.precision * 10 + (spec[i++] - '0');
    }
    if (i < size) s.type = spec[i++];
    if (i < size) lt_bad_spec(spec, size, kind, at);
    return s;
}

/* `text` laid out in the spec's width, fill and alignment; `left` names the default alignment. */
static void lt_put_aligned(lt_buf *b, const lt_spec *s, const char *text, size_t size, int64_t points, char align) {
    int64_t margin = s->width > points ? s->width - points : 0;
    int64_t left = align == '<' ? 0 : align == '>' ? margin : margin / 2;
    for (int64_t i = 0; i < left; i++) lt_buf_code(b, s->fill);
    lt_buf_put(b, text, size);
    for (int64_t i = left; i < margin; i++) lt_buf_code(b, s->fill);
}

/* `digits` (and what follows them, from `tail`) with a separator every `every` digits of the
 * integer part, right to left. */
static void lt_group(lt_buf *out, const char *digits, size_t size, char separator, int every) {
    size_t whole = 0;
    while (whole < size && digits[whole] >= '0' && digits[whole] <= '9') whole++;
    if (separator == 0) {
        lt_buf_put(out, digits, size);
        return;
    }
    for (size_t i = 0; i < whole; i++) {
        if (i > 0 && (whole - i) % (size_t)every == 0) lt_buf_put(out, &separator, 1);
        lt_buf_put(out, digits + i, 1);
    }
    lt_buf_put(out, digits + whole, size - whole);
}

/* A number: its sign, a prefix (`0x`), its digits grouped, laid out as the spec says. */
static void lt_put_number(lt_buf *b, const lt_spec *s, bool negative, const char *prefix, const char *digits, size_t size,
                          int every) {
    char sign[2] = {0, 0};
    if (negative) sign[0] = '-';
    else if (s->sign == '+') sign[0] = '+';
    else if (s->sign == ' ') sign[0] = ' ';
    lt_buf grouped = LT_BUF;
    lt_buf_puts(&grouped, prefix);
    size_t head = grouped.len;
    lt_group(&grouped, digits, size, s->grouping, every);
    char align = s->align == 0 ? '>' : s->align;
    if (align == '=' && s->fill == '0' && s->grouping != 0) {
        /* Zero padding is grouped too: `format(1234, '09,')` is `0,001,234`. */
        lt_buf zeros = LT_BUF;
        size_t whole = 0;
        while (whole < size && digits[whole] >= '0' && digits[whole] <= '9') whole++;
        lt_buf padded = LT_BUF;
        while ((int64_t)(strlen(sign) + grouped.len) < s->width) {
            lt_buf_put(&zeros, "0", 1);
            lt_buf_free(&padded);
            lt_buf_put(&padded, zeros.data, zeros.len);
            lt_buf_put(&padded, digits, size);
            grouped.len = head;
            lt_group(&grouped, padded.data, padded.len, s->grouping, every);
        }
        lt_buf_free(&zeros);
        lt_buf_free(&padded);
    }
    int64_t points = (int64_t)strlen(sign) + (int64_t)grouped.len;
    int64_t margin = s->width > points ? s->width - points : 0;
    if (align == '=') {
        lt_buf_puts(b, sign);
        lt_buf_put(b, grouped.data, head);
        for (int64_t i = 0; i < margin; i++) lt_buf_code(b, s->fill);
        lt_buf_put(b, grouped.data + head, grouped.len - head);
    } else {
        int64_t left = align == '<' ? 0 : align == '>' ? margin : margin / 2;
        for (int64_t i = 0; i < left; i++) lt_buf_code(b, s->fill);
        lt_buf_puts(b, sign);
        lt_buf_put(b, grouped.data, grouped.len);
        for (int64_t i = left; i < margin; i++) lt_buf_code(b, s->fill);
    }
    lt_buf_free(&grouped);
}

static void lt_format_integer(lt_buf *b, bool negative, uint64_t magnitude, const lt_spec *s, const char *spec, size_t size,
                              lt_at at) {
    char digits[80];
    const char *prefix = "";
    int every = 3;
    int n = 0;
    switch (s->type) {
    case 0:
    case 'd':
    case 'n':
        n = snprintf(digits, sizeof digits, "%llu", (unsigned long long)magnitude);
        break;
    case 'x':
    case 'X':
        n = snprintf(digits, sizeof digits, s->type == 'x' ? "%llx" : "%llX", (unsigned long long)magnitude);
        prefix = s->alternate ? (s->type == 'x' ? "0x" : "0X") : "";
        every = 4;
        break;
    case 'o':
        n = snprintf(digits, sizeof digits, "%llo", (unsigned long long)magnitude);
        prefix = s->alternate ? "0o" : "";
        every = 4;
        break;
    case 'b': {
        int k = 0;
        char reversed[70];
        do {
            reversed[k++] = (char)('0' + (magnitude & 1));
            magnitude >>= 1;
        } while (magnitude > 0);
        for (int i = 0; i < k; i++) digits[i] = reversed[k - 1 - i];
        n = k;
        prefix = s->alternate ? "0b" : "";
        every = 4;
        break;
    }
    case 'c': {
        if (negative || magnitude > 0x10FFFF) lt_panic(at, "OverflowError", "%c arg not in range(0x110000)");
        char bytes[4];
        int k = lt_encode((int32_t)magnitude, bytes);
        lt_put_aligned(b, s, bytes, (size_t)k, 1, s->align == 0 ? '>' : s->align);
        return;
    }
    default:
        lt_bad_spec(spec, size, "int", at);
    }
    if (s->grouping == ',' && every == 4) lt_bad_spec(spec, size, "int", at);
    lt_put_number(b, s, negative, prefix, digits, (size_t)n, every);
}

void lt_format_i64(lt_buf *b, int64_t value, const char *spec, size_t size, lt_at at) {
    if (size == 0) {
        lt_buf_i64(b, value);
        return;
    }
    lt_spec s = lt_parse_spec(spec, size, "int", at);
    if (s.type == 'e' || s.type == 'E' || s.type == 'f' || s.type == 'F' || s.type == 'g' || s.type == 'G' || s.type == '%') {
        lt_format_f64(b, (double)value, spec, size, at);
        return;
    }
    if (s.precision >= 0) lt_panic(at, "ValueError", "Precision not allowed in integer format specifier");
    uint64_t magnitude = value < 0 ? (uint64_t)0 - (uint64_t)value : (uint64_t)value;
    lt_format_integer(b, value < 0, magnitude, &s, spec, size, at);
}

void lt_format_u64(lt_buf *b, uint64_t value, const char *spec, size_t size, lt_at at) {
    if (size == 0) {
        lt_buf_u64(b, value);
        return;
    }
    lt_spec s = lt_parse_spec(spec, size, "int", at);
    if (s.type == 'e' || s.type == 'E' || s.type == 'f' || s.type == 'F' || s.type == 'g' || s.type == 'G' || s.type == '%') {
        lt_format_f64(b, (double)value, spec, size, at);
        return;
    }
    if (s.precision >= 0) lt_panic(at, "ValueError", "Precision not allowed in integer format specifier");
    lt_format_integer(b, false, value, &s, spec, size, at);
}

void lt_format_bool(lt_buf *b, bool value, const char *spec, size_t size, lt_at at) {
    if (size == 0) {
        lt_buf_bool(b, value);
        return;
    }
    lt_format_i64(b, value ? 1 : 0, spec, size, at);
}

void lt_format_none(lt_buf *b, const char *spec, size_t size, lt_at at) {
    (void)spec;
    if (size != 0) lt_panic(at, "TypeError", "unsupported format string passed to NoneType.__format__");
    lt_buf_puts(b, "None");
}

/* The digits of `magnitude` rounded to `precision` significant digits, and the exponent of the
 * first: as `%.*e` writes them, from which 'g' and the default are laid out. */
static int lt_round_digits(double magnitude, int precision, char digits[400], int *exponent) {
    char text[440];
    snprintf(text, sizeof text, "%.*e", precision - 1, magnitude);
    int n = 0;
    const char *p = text;
    for (; *p != 'e'; p++) {
        if (*p != '.') digits[n++] = *p;
    }
    digits[n] = '\0';
    *exponent = atoi(p + 1);
    return n;
}

/* 'g' and the default with a precision: positional when -4 <= exponent < threshold, trailing
 * zeros dropped unless `keep`, and ".0" added to a whole number when `dot_zero`. */
static void lt_general(lt_buf *out, double magnitude, int precision, bool keep, bool dot_zero, bool upper, int threshold) {
    char digits[400];
    int exponent;
    int n = lt_round_digits(magnitude, precision, digits, &exponent);
    if (!keep) {
        while (n > 1 && digits[n - 1] == '0') n--;
    }
    if (exponent < -4 || exponent >= threshold) {
        lt_buf_put(out, digits, 1);
        if (n > 1 || keep) lt_buf_put(out, ".", 1);
        if (n > 1) lt_buf_put(out, digits + 1, (size_t)(n - 1));
        char tail[8];
        int t = snprintf(tail, sizeof tail, "%c%c%02d", upper ? 'E' : 'e', exponent < 0 ? '-' : '+', exponent < 0 ? -exponent : exponent);
        lt_buf_put(out, tail, (size_t)t);
        return;
    }
    if (exponent < 0) {
        lt_buf_put(out, "0.", 2);
        for (int i = 0; i < -exponent - 1; i++) lt_buf_put(out, "0", 1);
        lt_buf_put(out, digits, (size_t)n);
        return;
    }
    int whole = exponent + 1;
    if (n <= whole) {
        lt_buf_put(out, digits, (size_t)n);
        for (int i = n; i < whole; i++) lt_buf_put(out, "0", 1);
        if (dot_zero) lt_buf_put(out, ".0", 2);
        else if (keep && precision > whole) {
            lt_buf_put(out, ".", 1);
            for (int i = whole; i < precision; i++) lt_buf_put(out, "0", 1);
        }
        return;
    }
    lt_buf_put(out, digits, (size_t)whole);
    lt_buf_put(out, ".", 1);
    lt_buf_put(out, digits + whole, (size_t)(n - whole));
}

void lt_format_f64(lt_buf *b, double value, const char *spec, size_t size, lt_at at) {
    if (size == 0) {
        lt_buf_f64(b, value);
        return;
    }
    lt_spec s = lt_parse_spec(spec, size, "float", at);
    if (s.type == 'n') s.type = 'g';
    bool negative = signbit(value) != 0 && !isnan(value);
    double magnitude = fabs(value);
    lt_buf digits = LT_BUF;
    bool upper = s.type == 'E' || s.type == 'F' || s.type == 'G';
    if (isinf(magnitude) || isnan(magnitude)) {
        lt_buf_puts(&digits, isnan(magnitude) ? (upper ? "NAN" : "nan") : (upper ? "INF" : "inf"));
        if (s.type == '%') lt_buf_put(&digits, "%", 1);
    } else {
        int precision = s.precision < 0 ? 6 : (int)s.precision;
        char text[440];
        switch (s.type) {
        case 'f':
        case 'F':
        case '%': {
            double shown = s.type == '%' ? magnitude * 100.0 : magnitude;
            int n = snprintf(text, sizeof text, s.alternate && precision == 0 ? "%#.*f" : "%.*f", precision, shown);
            lt_buf_put(&digits, text, (size_t)n);
            if (s.type == '%') lt_buf_put(&digits, "%", 1);
            break;
        }
        case 'e':
        case 'E': {
            int n = snprintf(text, sizeof text, s.alternate && precision == 0 ? "%#.*e" : "%.*e", precision, magnitude);
            if (upper) {
                for (int i = 0; i < n; i++) if (text[i] == 'e') text[i] = 'E';
            }
            lt_buf_put(&digits, text, (size_t)n);
            break;
        }
        case 'g':
        case 'G': {
            int p = precision == 0 ? 1 : precision;
            if (magnitude == 0.0) {
                lt_buf_puts(&digits, s.alternate ? "0." : "0");
                if (s.alternate) for (int i = 1; i < p; i++) lt_buf_put(&digits, "0", 1);
            } else {
                lt_general(&digits, magnitude, p, s.alternate, false, upper, p);
            }
            break;
        }
        case 0:
            if (s.precision < 0) {
                lt_buf_f64(&digits, magnitude);
            } else {
                int p = precision == 0 ? 1 : precision;
                if (magnitude == 0.0) lt_buf_puts(&digits, "0.0");
                else lt_general(&digits, magnitude, p, s.alternate, true, false, p - 1);
            }
            break;
        default:
            lt_buf_free(&digits);
            lt_bad_spec(spec, size, "float", at);
        }
    }
    if (negative && s.no_negative_zero) {
        bool zero = true;
        for (size_t i = 0; i < digits.len; i++) {
            if (digits.data[i] >= '1' && digits.data[i] <= '9') zero = false;
            if (digits.data[i] == 'e' || digits.data[i] == 'E') break;
        }
        if (zero) negative = false;
    }
    lt_put_number(b, &s, negative, "", digits.data == NULL ? "" : digits.data, digits.len, 3);
    lt_buf_free(&digits);
}

void lt_format_str(lt_buf *b, const lt_str *value, const char *spec, size_t size, lt_at at) {
    if (size == 0) {
        lt_buf_str(b, value);
        return;
    }
    lt_spec s = lt_parse_spec(spec, size, "str", at);
    if ((s.type != 0 && s.type != 's') || s.sign != 0 || s.align == '=' || s.alternate || s.grouping != 0) {
        lt_bad_spec(spec, size, "str", at);
    }
    int64_t points = value->length;
    int64_t bytes = value->size;
    if (s.precision >= 0 && s.precision < points) {
        points = s.precision;
        bytes = lt_offset(value, points);
    }
    lt_put_aligned(b, &s, value->bytes, (size_t)bytes, points, s.align == 0 ? '<' : s.align);
}

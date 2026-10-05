fn top_words(text: str, k: int) -> [(str, int)]:
    cleaned = "".join([c if c.isalpha() else " " for c in text.lower()])
    var counts: {str: int} = {}
    for w in cleaned.split(" "):
        if w != "":
            counts[w] = counts.get(w, 0) + 1
    var entries: [(int, str)] = []
    for w, n in counts.items():
        entries.append((-n, w))
    entries.sort()
    top = entries[0:min(k, len(entries))]
    return [(w, -n) for n, w in top]

test "most frequent words come first":
    text = "The cat and the dog and THE bird"
    assert top_words(text, 2) == [("the", 3), ("and", 2)]

test "ties are broken alphabetically and k may exceed the word count":
    assert top_words("b a b a c", 5) == [("a", 2), ("b", 2), ("c", 1)]

test "punctuation is ignored":
    assert top_words("Hello, hello! World.", 1) == [("hello", 2)]

test "empty text has no words":
    assert top_words("", 3) == []

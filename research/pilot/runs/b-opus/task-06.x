# Task 6: the k most frequent lowercase words, most frequent first.

fn words(text: str) -> [str]:
    var out: [str] = []
    var cur = ""
    for c in text.lower():
        if c.isalpha():
            cur += c
        elif cur != "":
            out.append(cur)
            cur = ""
    if cur != "":
        out.append(cur)
    return out

fn top_words(text: str, k: int) -> [(str, int)]:
    var counts: {str: int} = {}
    for w in words(text):
        counts[w] = counts.get(w, 0) + 1
    # Most frequent first; ties broken alphabetically so the result is deterministic.
    ranked = sorted(counts.items(), key=lambda p: (-p[1], p[0]))
    return ranked[:k]

test "returns the k most frequent words with counts":
    text = "The cat and the hat. And THE bat!"
    assert top_words(text, 2) == [("the", 3), ("and", 2)]

test "ties are ordered alphabetically":
    assert top_words("b a c", 2) == [("a", 1), ("b", 1)]

test "k larger than the number of words":
    assert top_words("one one", 5) == [("one", 2)]

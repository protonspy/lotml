# Task 6: k most frequent lowercase words

fn words(text: str) -> [str]:
    var result: [str] = []
    var current = ""
    for c in text.lower():
        if c.isalpha():
            current += c
        elif current != "":
            result.append(current)
            current = ""
    if current != "":
        result.append(current)
    return result

fn top_words(text: str, k: int) -> [(str, int)]:
    var counts: {str: int} = {}
    for w in words(text):
        counts[w] = counts.get(w, 0) + 1
    # most frequent first; ties broken alphabetically
    ranked = sorted(counts.items(), key=p => (-p[1], p[0]))
    return ranked[0:min(k, len(ranked))]

test "returns the k most frequent words, most frequent first":
    text = "The cat and the dog. THE bird, and the fish!"
    assert top_words(text, 2) == [("the", 4), ("and", 2)]

test "k larger than the number of distinct words":
    assert top_words("b a b", 10) == [("b", 2), ("a", 1)]

test "empty text":
    assert len(top_words("", 3)) == 0

fn top_words(text: str, k: int) -> [(str, int)]:
    var counts: {str: int} = {}
    for w in text.split():
        if w.isalpha():
            key = w.lower()
            counts[key] = counts.get(key, 0) + 1
    pairs = sorted(counts.items(), key=p => -p[1])
    return pairs[:k]

test "top words":
    text = "the cat and the dog and the bird"
    assert top_words(text, 2) == [("the", 3), ("and", 2)]

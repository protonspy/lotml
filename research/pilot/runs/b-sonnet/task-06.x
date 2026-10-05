fn top_words(text: str, k: int) -> [(str, int)]:
    var counts: {str: int} = {}
    for word in text.lower().split():
        if word != "":
            counts[word] = counts.get(word, 0) + 1
    pairs = sorted(counts.items(), key=lambda p: (-p[1], p[0]))
    end = max(0, min(k, len(pairs)))
    return pairs[0:end]

test "returns the most frequent words first":
    result = top_words("the cat and the dog and the bird", 2)
    assert result == [("the", 3), ("and", 2)]

test "words are lowercased before counting":
    result = top_words("Go go GO stop Stop", 2)
    assert result == [("go", 3), ("stop", 2)]

test "ties are broken alphabetically":
    result = top_words("b a c", 2)
    assert result == [("a", 1), ("b", 1)]

test "k larger than the number of distinct words":
    result = top_words("one two two", 10)
    assert result == [("two", 2), ("one", 1)]

test "empty text has no words":
    result = top_words("", 3)
    assert len(result) == 0

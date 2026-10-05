fn most_frequent_words(text: str, k: int) -> [(str, int)]:
    words = text.lower().split(" ")
    var counts: {str: int} = {}
    for word in words:
        if word != "":
            current = counts.get(word, 0)
            counts[word] = current + 1
    items = counts.items()
    sorted_items = sorted(items, key=p => -p[1])
    return sorted_items[:k]

test "most frequent words":
    text = "hello world hello foo bar hello"
    result = most_frequent_words(text, 2)
    assert len(result) == 2
    assert result[0][0] == "hello"
    assert result[0][1] == 3
    assert result[1][0] == "world" or result[1][0] == "foo"

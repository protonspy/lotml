fn top_k_words(text: str, k: int) -> [(str, int)]:
    words = text.lower().split()
    var counts: {str: int} = {}
    for word in words:
        counts[word] = counts.get(word, 0) + 1

    items = []
    for word, count in counts.items():
        items.append((word, count))

    sorted_items = sorted(items, key=lambda p: (0 - p[1]))
    return sorted_items[:k]

test "top k words":
    text = "hello world hello test test test"
    result = top_k_words(text, 2)
    assert result[0][0] == "test" and result[0][1] == 3
    assert result[1][0] == "hello" and result[1][1] == 2

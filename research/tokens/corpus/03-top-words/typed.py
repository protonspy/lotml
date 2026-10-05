def top_words(text: str, k: int) -> list[tuple[str, int]]:
    counts: dict[str, int] = {}
    for w in text.split():
        if w.isalpha():
            key = w.lower()
            counts[key] = counts.get(key, 0) + 1
    pairs = sorted(counts.items(), key=lambda p: -p[1])
    return pairs[:k]


def test_top_words() -> None:
    text = "the cat and the dog and the bird"
    assert top_words(text, 2) == [("the", 3), ("and", 2)]

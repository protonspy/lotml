import heapq


def shortest_path(
    graph: dict[str, list[tuple[str, int]]], start: str, goal: str
) -> int | None:
    dist: dict[str, int] = {start: 0}
    queue: list[tuple[int, str]] = [(0, start)]
    while queue:
        d, node = heapq.heappop(queue)
        if node == goal:
            return d
        if d > dist.get(node, d):
            continue
        for neighbor, weight in graph.get(node, []):
            nd = d + weight
            if nd < dist.get(neighbor, nd + 1):
                dist[neighbor] = nd
                heapq.heappush(queue, (nd, neighbor))
    return None


def test_shortest_path() -> None:
    graph = {"a": [("b", 1), ("c", 4)], "b": [("c", 2)], "c": []}
    assert shortest_path(graph, "a", "c") == 3
    assert shortest_path(graph, "c", "a") is None

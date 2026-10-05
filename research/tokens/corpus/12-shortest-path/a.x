fn shortest_path(graph: {str: [(str, int)]}, start: str, goal: str) -> int?:
    var dist: {str: int} = {start: 0}
    var queue = Heap([(0, start)])
    while len(queue) > 0:
        d, node = queue.pop_min()
        if node == goal:
            return d
        if d > dist.get(node, d):
            continue
        for neighbor, weight in graph.get(node, []):
            nd = d + weight
            if nd < dist.get(neighbor, nd + 1):
                dist[neighbor] = nd
                queue.push((nd, neighbor))
    return none

test "shortest path":
    graph = {"a": [("b", 1), ("c", 4)], "b": [("c", 2)], "c": []}
    assert shortest_path(graph, "a", "c") == 3
    assert shortest_path(graph, "c", "a") == none

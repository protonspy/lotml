"""The agent harness's report: pass@k by Chen et al.'s unbiased estimator, the Wilson interval of
pass@1, and the Markdown tables (specs/agent-harness/ R4.2)."""


def pass_at_k(n: int, c: int, k: int) -> float:
    """The chance that at least one of `k` runs drawn from `n`, `c` of them passing, passes."""
    raise NotImplementedError


def wilson(passed: int, runs: int, z: float = 1.96) -> tuple[float, float]:
    """The Wilson score interval of a pass rate; 95% for the default `z`."""
    raise NotImplementedError

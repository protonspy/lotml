import base64
import json
import pickle
import zlib

import pytest

from lotml_harness.tasks.extract import Untranslatable
from lotml_harness.tasks.livecodebench import decode_private, from_record


def encode_private(cases: list[dict]) -> str:
    return base64.b64encode(zlib.compress(pickle.dumps(json.dumps(cases)))).decode()


def record(**overrides) -> dict:
    base = {
        "question_title": "Max Sum",
        "question_content": 'Return the maximum sum.\nUse """quotes""" and \\n.',
        "platform": "leetcode",
        "question_id": "3500",
        "contest_id": "weekly-contest-430",
        "contest_date": "2025-01-04T00:00:00",
        "starter_code": (
            "class Solution:\n    def maxSum(self, nums: List[int], k: int) -> int:\n        "
        ),
        "difficulty": "medium",
        "public_test_cases": json.dumps(
            [{"input": "[1, 2, 3]\n2", "output": "5", "testtype": "functional"}]
        ),
        "private_test_cases": encode_private(
            [{"input": "[4]\n1", "output": "4", "testtype": "functional"}]
        ),
        "metadata": json.dumps({"func_name": "maxSum"}),
    }
    return base | overrides


def test_a_leetcode_problem_becomes_a_typed_function():
    task = from_record(record())
    assert task.id == "livecodebench/3500"
    assert task.name == "maxSum"
    assert task.signature("b") == "fn maxSum(nums: [int], k: int) -> int:"
    assert [(c.args, c.expected, c.compare) for c in task.tests] == [
        ([[1, 2, 3], 2], 5, "eq"),
        ([[4], 1], 4, "eq"),
    ]
    assert task.meta == {
        "platform": "leetcode",
        "difficulty": "medium",
        "date": "2025-01-04",
    }


def test_the_statement_is_a_safe_docstring():
    prompt = from_record(record()).prompt("b")
    body = prompt.split("\n", 1)[1]
    assert body.count('"""') == 2
    assert "\\\\n" in body
    assert "    Return the maximum sum." in body


def test_a_standard_input_problem_reads_its_input_as_a_string():
    task = from_record(
        record(
            platform="atcoder",
            question_id="abc390_a",
            starter_code="",
            public_test_cases=json.dumps(
                [{"input": "3\n1 2 3\n", "output": "6\n", "testtype": "stdin"}]
            ),
            private_test_cases=encode_private([]),
            metadata="{}",
        )
    )
    assert task.signature("b") == "fn solve(input: str) -> str:"
    assert "return what the program would print" in task.doc
    assert [(c.args, c.expected, c.compare) for c in task.tests] == [
        (["3\n1 2 3\n"], "6\n", "lines")
    ]


def test_oversized_cases_are_left_out_and_a_task_needs_one():
    big = "9" * 30_000
    with pytest.raises(Untranslatable):
        from_record(
            record(
                public_test_cases=json.dumps(
                    [{"input": f"[{big}]\n1", "output": "1", "testtype": "functional"}]
                ),
                private_test_cases=encode_private([]),
            )
        )


def test_unsupported_signatures_are_refused():
    starter = "class Solution:\n    def f(self, root: Optional[TreeNode]) -> int:\n        "
    with pytest.raises(Untranslatable):
        from_record(record(starter_code=starter, metadata='{"func_name": "f"}'))


def test_private_cases_decode_only_plain_data():
    class Evil:
        def __reduce__(self):
            return (print, ("pwned",))

    payload = base64.b64encode(zlib.compress(pickle.dumps(Evil()))).decode()
    with pytest.raises(pickle.UnpicklingError):
        decode_private(payload)
    assert decode_private(encode_private([{"a": 1}])) == [{"a": 1}]

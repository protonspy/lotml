"""The corpus pipeline: the rules first, a model with the compiler for the rest, and nothing in
the corpus that does not pass its tests."""

from lotml_harness.corpus import pipeline
from lotml_harness.experiments.models import Completion, Model
from lotml_harness.experiments.phase1 import Lotml
from lotml_harness.tasks import Case, Task, types

INT = types.Prim("int")


def task(canonical: str, tests, name: str = "f", task_id: str = "t/1") -> Task:
    return Task(
        id=task_id,
        source="t",
        name=name,
        params=[("n", INT)],
        returns=INT,
        doc="",
        tests=tests,
        canonical=canonical,
    )


DOUBLE = task("def f(n):\n    return n * 2\n", [Case([1], 2), Case([3], 6)])
GUARDED = task(
    "def f(n):\n    try:\n        return n * 2\n    except Exception:\n        return 0\n",
    [Case([2], 4)],
    task_id="t/2",
)


class Scripted(Model):
    def __init__(self, answers):
        super().__init__("scripted", "Test")
        self.answers = list(answers)
        self.seen = []

    def chat(self, system, messages):
        self.seen.append((system, [m["content"] for m in messages]))
        return Completion(self.answers.pop(0), input_tokens=10, output_tokens=5, seconds=0.1)


def test_a_translation_the_rules_pass_asks_no_model():
    model = Scripted([])
    record = pipeline.translate(DOUBLE, model, Lotml())
    assert record["origin"] == "rules"
    assert record["code"] == "fn f(n: int) -> int:\n    return n * 2\n"
    assert model.seen == []


def test_what_the_rules_cannot_write_goes_to_the_model_with_the_reason():
    model = Scripted(["```lotml\nfn f(n: int) -> int:\n    return n * 2\n```"])
    record = pipeline.translate(GUARDED, model, Lotml())
    assert record["origin"] == "model" and record["green"] == 1
    system, (user,) = model.seen[0]
    assert "You translate Python into lotml" in system and "# lotml language reference" in system
    assert "`fn f(n: int) -> int`" in user and "except Exception" in user
    assert "A mechanical translation stopped at: an exception handler." in user


def test_a_failed_answer_is_answered_with_the_compiler_s_feedback():
    model = Scripted(
        [
            "```lotml\nfn f(n: int) -> int:\n    x = 1\n    x = 2\n    return n * 2\n```",
            "```lotml\nfn f(n: int) -> int:\n    return n * 2\n```",
        ]
    )
    record = pipeline.translate(GUARDED, model, Lotml())
    assert record["green"] == 2
    _, second = model.seen[1]
    assert "error[E0301]" in second[-1]


def test_a_task_no_one_translates_stays_out_of_the_corpus():
    wrong = "```lotml\nfn f(n: int) -> int:\n    return n\n```"
    record = pipeline.translate(GUARDED, Scripted([wrong] * 3), Lotml())
    assert record["origin"] is None and record["code"] is None
    assert pipeline.corpus([GUARDED], [record], Lotml()) == []


def test_the_corpus_keeps_each_program_with_its_tests_as_a_test_block():
    record = pipeline.translate(DOUBLE, None, Lotml())
    (entry,) = pipeline.corpus([DOUBLE], [record], Lotml())
    assert entry["tests"] == 2
    assert entry["lotml"].endswith('\n\ntest "f":\n    assert f(1) == 2\n    assert f(3) == 6\n')
    assert entry["python"] == DOUBLE.canonical


def test_the_report_counts_each_source_by_how_it_was_translated():
    model = Scripted(["```lotml\nfn f(n: int) -> int:\n    return n * 2\n```"])
    judge = Lotml()
    chosen = [pipeline.translate(DOUBLE, None, judge), pipeline.translate(GUARDED, model, judge)]
    entries = pipeline.corpus([DOUBLE, GUARDED], chosen, judge)
    text = pipeline.markdown(pipeline.summarize([DOUBLE, GUARDED], chosen, entries), entries)
    assert "| t | 2 | 1 | 1 | 0 | 0 | 2 | 2 |" in text
    assert "Every one of the 2 programs" in text

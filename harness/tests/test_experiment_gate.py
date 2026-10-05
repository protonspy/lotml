import json

from lotml_harness.experiments import gate


def answer(model, task, variant, passed, parsed=True, leaks=()):
    return {
        "model": model,
        "family": "f",
        "task": task,
        "variant": variant,
        "parse_error": None if parsed else "x",
        "leaks": list(leaks),
        "violations": [],
        "passed": passed,
        "passed_python": passed,
        "semantics_dependent": False,
        "error": None,
    }


def edit(model, task, form, fixed):
    turn = {"outcome": "pass" if fixed else "tests fail", "searched": 0.1, "idioms": []}
    return {
        "model": model,
        "family": "f",
        "task": task,
        "form": form,
        "turns": [turn | {"tolerant": turn["outcome"]}],
        "fixed": fixed,
        "fixed_first": fixed,
        "fixed_first_tolerant": fixed,
        "error": None,
    }


def answers(n, b_wins=0, a_wins=0, unparsed=0, leaked=0):
    rows = []
    for i in range(n):
        a_pass = i < a_wins or i >= a_wins + b_wins
        b_pass = not (i < a_wins)
        rows.append(answer("claude-sonnet", f"t{i}", "a", a_pass))
        rows.append(
            answer(
                "claude-sonnet",
                f"t{i}",
                "b",
                b_pass,
                parsed=i >= unparsed,
                leaks=["def"] if i < leaked else (),
            )
        )
    return rows


def edits(n, indented_wins=0, braces_wins=0):
    rows = []
    for i in range(n):
        rows.append(edit("m", f"t{i}", "indented", not (i < braces_wins)))
        rows.append(
            edit("m", f"t{i}", "braces", i < braces_wins or i >= braces_wins + indented_wins)
        )
    return rows


def results(criteria):
    return {c.name: c.passed for c in criteria}


def test_the_gate_passes_when_every_criterion_holds():
    criteria = gate.evaluate(answers(170, b_wins=10), edits(170, indented_wins=3))
    assert all(results(criteria).values())
    assert "The gate passes." in gate.markdown(criteria)


def test_parse_rate_and_leakage_are_held_to_the_frontier_model():
    criteria = results(gate.evaluate(answers(170, unparsed=10, leaked=1), edits(170)))
    assert not criteria["claude-sonnet parses at least 95% in variant B"]
    assert not criteria["claude-sonnet leaks no Python syntax in variant B"]


def test_a_significantly_worse_form_fails_the_gate():
    criteria = results(gate.evaluate(answers(170), edits(170, braces_wins=12)))
    assert not criteria["the indented form is not worse than braces"]
    criteria = results(gate.evaluate(answers(170, a_wins=12), edits(170)))
    assert not criteria["variant B is not worse than variant A"]


def test_too_few_pairs_fail_the_comparison():
    criteria = results(gate.evaluate(answers(100, b_wins=5), edits(100)))
    assert not criteria["variant B is not worse than variant A"]


def test_a_missing_frontier_model_fails_its_criteria():
    criteria = gate.evaluate([], [])
    assert not any(c.passed for c in criteria)
    assert "does not pass" in gate.markdown(criteria)


def test_rows_keep_successful_answers_over_failed_attempts(tmp_path):
    lines = [
        {"model": "m", "task": "t", "variant": "a", "error": "quota"},
        {"model": "m", "task": "t", "variant": "a", "error": None, "x": 1},
        {"model": "m", "task": "u", "variant": "a", "error": "quota"},
    ]
    (tmp_path / "m.jsonl").write_text("\n".join(json.dumps(r) for r in lines))
    (tmp_path / "tasks.jsonl").write_text("not json\n")
    assert gate.rows(tmp_path) == [lines[1]]

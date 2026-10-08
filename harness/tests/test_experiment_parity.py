"""The parity suite: every corpus program's `test` blocks on the Python and the LLVM target, which
must report the same."""

import json
import shutil
import subprocess

import pytest

from lotml_harness.experiments import parity

RIGHT = 'fn double(n: int) -> int:\n    return n * 2\n\ntest "double":\n    assert double(2) == 4\n'
WRONG = 'fn double(n: int) -> int:\n    return n + 2\n\ntest "double":\n    assert double(2) == 5\n'


def ran(stdout: str, code: int = 0) -> subprocess.CompletedProcess:
    return subprocess.CompletedProcess([], code, stdout, "")


def report(*tests: dict) -> str:
    rows = [{**t, "file": "p0.lotml"} for t in tests]
    return json.dumps({"version": 1, "tests": rows, "summary": {}}) + "\n"


PASS = {"name": "double", "outcome": "pass"}
FAIL = {"name": "double", "outcome": "fail", "expression": "double(2) == 5", "line": 5}


def test_the_same_report_on_both_targets_is_the_same_outcome():
    outcome = parity.compare("t/1", ran(report(PASS)), ran(report(PASS)))
    assert (outcome.task, outcome.verdict) == ("t/1", "same")


def test_where_a_file_was_written_does_not_count():
    other = report(PASS).replace("p0.lotml", "elsewhere/p0.lotml")
    assert parity.compare("t/1", ran(report(PASS)), ran(other)).verdict == "same"


def test_a_different_outcome_is_named_with_both_rows():
    outcome = parity.compare("t/1", ran(report(PASS)), ran(report(FAIL), 1))
    assert outcome.verdict == "differs"
    assert '"outcome": "pass"' in outcome.detail and '"outcome": "fail"' in outcome.detail


def test_a_python_import_refused_on_the_llvm_target_is_set_apart():
    refused = "prog.lotml:1:1: error[E0401]: `textwrap` is a Python module, and …\n"
    assert parity.compare("t/1", ran(report(PASS)), ran(refused, 1)).verdict == "refused"


def test_a_program_the_llvm_backend_does_not_compile_names_why():
    unsupported = "prog.lotml:3:5: error[E0402]: `--target llvm` does not compile this yet\n"
    outcome = parity.compare("t/1", ran(report(PASS)), ran(unsupported, 1))
    assert outcome.verdict == "not compiled"
    assert "E0402" in outcome.detail


def test_a_native_program_that_did_not_report_is_a_failure_of_its_own():
    assert parity.compare("t/1", ran(report(PASS)), ran("", 2)).verdict == "no report"


def test_a_real_program_reports_the_same_on_both_targets_even_when_it_fails():
    outcomes = parity.suite([{"task": "t/1", "lotml": RIGHT}, {"task": "t/2", "lotml": WRONG}])
    assert [(o.task, o.verdict) for o in outcomes] == [("t/1", "same"), ("t/2", "same")]


def test_the_gate_passes_only_when_every_program_not_refused_reports_the_same():
    same = parity.Outcome("t/1", "same", "")
    refused = parity.Outcome("t/2", "refused", "")
    differs = parity.Outcome("t/3", "differs", "a row")
    assert "The suite passes" in parity.markdown([same, refused])
    assert "The suite does not pass" in parity.markdown([same, differs])


def outcome(task: str, verdict: str = "same", detail: str = "") -> parity.Outcome:
    return parity.Outcome(task, verdict, detail)


def test_the_floor_is_the_programs_reporting_the_same_and_their_count():
    floor = parity.floor_of([outcome("b"), outcome("a"), outcome("c", "differs", "a row")])
    assert floor == parity.Floor(2, ["a", "b"])
    assert parity.load_floor(parity.dump_floor(floor)) == floor


def test_a_run_fails_the_floor_only_when_a_program_leaves_it_or_the_count_drops():
    floor = parity.Floor(2, ["a", "b"])
    assert parity.problems(floor, [outcome("a"), outcome("b"), outcome("c")]) == []
    left = parity.problems(floor, [outcome("a"), outcome("b", "differs", "a row")])
    assert any(p.startswith("`b` left the floor: differs a row") for p in left)
    assert any("fewer than the floor's 2" in p for p in left)
    missing = parity.problems(floor, [outcome("a"), outcome("c")])
    assert missing == ["`b` did not run"]


def test_the_diff_names_what_left_the_floor_and_what_joined_it():
    floor = parity.Floor(2, ["a", "b"])
    run = [outcome("a"), outcome("b", "differs", "a row"), outcome("c")]
    assert parity.diff(floor, run) == (["b"], ["c"])


def test_the_floor_is_never_updated_from_a_filtered_run(monkeypatch):
    ran = []
    monkeypatch.setattr(parity, "suite", lambda entries: ran.append(entries) or [])
    with pytest.raises(SystemExit, match="whole run"):
        parity.main(["floor", "update", "--only", "humaneval/"])
    assert ran == [], "nothing ran"


def test_floor_check_fails_when_a_program_left_and_passes_when_none_did(monkeypatch, tmp_path):
    path = tmp_path / "floor.json"
    path.write_text(parity.dump_floor(parity.Floor(1, ["t/1"])), encoding="utf-8")
    monkeypatch.setattr(parity, "FLOOR", path)
    monkeypatch.setattr(parity, "entries", lambda only: [{"task": "t/1", "lotml": RIGHT}])
    monkeypatch.setattr(parity, "suite", lambda entries: [outcome("t/1", "differs", "a row")])
    assert parity.main(["floor", "check"]) == 1
    monkeypatch.setattr(parity, "suite", lambda entries: [outcome("t/1"), outcome("t/2")])
    assert parity.main(["floor", "check"]) == 0


def test_floor_update_writes_the_floor_of_a_whole_run(monkeypatch, tmp_path):
    path = tmp_path / "parity" / "floor.json"
    monkeypatch.setattr(parity, "FLOOR", path)
    monkeypatch.setattr(parity, "entries", lambda only: [])
    monkeypatch.setattr(parity, "suite", lambda entries: [outcome("t/2"), outcome("t/1")])
    assert parity.main(["floor", "update"]) == 0
    assert parity.load_floor(path.read_text(encoding="utf-8")) == parity.Floor(2, ["t/1", "t/2"])


def test_the_committed_floor_is_well_formed_and_counts_its_programs():
    floor = parity.load_floor(parity.FLOOR.read_text(encoding="utf-8"))
    assert floor.count == len(floor.same) == len(set(floor.same))
    assert floor.same == sorted(floor.same)


def test_refused_and_not_compiled_programs_are_grouped_by_their_message():
    grouped = parity.unsupported(
        [
            outcome("a", "refused", "error[E0401]: `textwrap` is a Python module"),
            outcome("c", "not compiled", "error[E0402]: not yet"),
            outcome("b", "not compiled", "error[E0402]: not yet"),
            outcome("d"),
        ]
    )
    assert grouped == {
        "refused": {"error[E0401]: `textwrap` is a Python module": ["a"]},
        "not compiled": {"error[E0402]: not yet": ["b", "c"]},
    }


def test_a_native_build_gets_what_finds_clang_and_its_cache_and_nothing_else(monkeypatch):
    monkeypatch.setenv("LOTML_CLANG", "/opt/llvm/bin/clang")
    monkeypatch.setenv("PATH", "/usr/bin")
    monkeypatch.setenv("OPENROUTER_API_KEY", "a secret")
    monkeypatch.setenv("PYTHONPATH", "/somewhere")
    env = parity.native_environment()
    assert env["LOTML_CLANG"] == "/opt/llvm/bin/clang"
    assert env["PATH"] == "/usr/bin"
    assert "OPENROUTER_API_KEY" not in env and "PYTHONPATH" not in env
    assert {k.upper() for k in env} <= set(parity.NATIVE_BUILD)


def test_a_program_leaves_the_floor_only_with_a_recorded_reason():
    old = parity.Floor(2, ["a", "b"])
    run = [outcome("a"), outcome("b", "differs", "a row"), outcome("c")]
    with pytest.raises(SystemExit, match="1 programs would leave the floor"):
        parity.updated(old, run, None)
    new = parity.updated(old, run, "b needs the new runtime")
    assert new == parity.Floor(2, ["a", "c"], {"b": "b needs the new runtime"})
    rejoined = parity.updated(new, [outcome("a"), outcome("b"), outcome("c")], None)
    assert rejoined == parity.Floor(3, ["a", "b", "c"], {})


def test_a_program_taken_out_by_hand_without_a_reason_fails_the_check():
    before = parity.Floor(2, ["a", "b"])
    assert parity.unexplained(before, parity.Floor(1, ["a"])) == [
        "`b` was taken out of the floor without a recorded reason"
    ]
    assert parity.unexplained(before, parity.Floor(1, ["a"], {"b": "a reason"})) == []
    assert parity.unexplained(before, parity.Floor(3, ["a", "b", "c"])) == []


def test_floor_check_since_a_revision_holds_the_floor_to_it(monkeypatch, tmp_path):
    path = tmp_path / "floor.json"
    path.write_text(parity.dump_floor(parity.Floor(1, ["t/1"])), encoding="utf-8")
    monkeypatch.setattr(parity, "FLOOR", path)
    monkeypatch.setattr(parity, "floor_at", lambda ref: parity.Floor(2, ["t/1", "t/2"]))
    monkeypatch.setattr(parity, "entries", lambda only: [])
    monkeypatch.setattr(parity, "suite", lambda entries: [outcome("t/1")])
    assert parity.main(["floor", "check", "--since", "origin/main"]) == 1
    assert parity.main(["floor", "check"]) == 0


def test_the_floor_at_a_revision_before_it_existed_is_empty_and_a_bad_revision_is_an_error():
    git = shutil.which("git")
    if git is None:
        pytest.skip("no git")
    shallow = subprocess.run(  # noqa: S603 - git, on this repository
        [git, "rev-parse", "--is-shallow-repository"],
        cwd=parity.ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    if shallow.stdout.strip() == "true":
        pytest.skip("a shallow clone has no first commit")
    first = subprocess.run(  # noqa: S603 - git, on this repository
        [git, "rev-list", "--max-parents=0", "HEAD"],
        cwd=parity.ROOT,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.split()[0]
    assert parity.floor_at(first) == parity.Floor(0, [])
    with pytest.raises(SystemExit, match="cannot read the floor"):
        parity.floor_at("no-such-revision")


def test_a_reason_of_blanks_is_no_reason():
    with pytest.raises(SystemExit, match="would leave the floor"):
        parity.updated(parity.Floor(1, ["a"]), [outcome("a", "differs", "a row")], "   ")


def test_a_revision_that_looks_like_an_option_is_refused():
    with pytest.raises(SystemExit, match="not a revision"):
        parity.floor_at("--output=elsewhere")


def test_the_native_build_runs_in_its_own_environment_and_python_in_the_harness_s(
    monkeypatch, tmp_path
):
    calls = []

    def fake(args, directory, timeout=0, env=None):
        calls.append((args, env))
        return ran(report(PASS))

    monkeypatch.setattr(parity, "compiler", fake)
    monkeypatch.setenv("OPENROUTER_API_KEY", "a secret")
    assert parity.run_both({"task": "t/1", "lotml": RIGHT}, tmp_path).verdict == "same"
    (python_args, python_env), (native_args, native_env) = calls
    assert "--target" not in python_args and python_env is None
    assert native_args[-3:-1] == ["--target", "llvm"]
    assert native_env == parity.native_environment() and "OPENROUTER_API_KEY" not in native_env


def test_a_filtered_check_holds_only_the_filtered_programs_of_the_floor(monkeypatch, tmp_path):
    path = tmp_path / "floor.json"
    path.write_text(parity.dump_floor(parity.Floor(2, ["humaneval/1", "mbpp/1"])), encoding="utf-8")
    monkeypatch.setattr(parity, "FLOOR", path)
    monkeypatch.setattr(parity, "entries", lambda only: [])
    monkeypatch.setattr(parity, "suite", lambda entries: [outcome("humaneval/1")])
    assert parity.main(["floor", "check", "--only", "humaneval/"]) == 0
    assert parity.main(["floor", "check"]) == 1


def test_a_floor_whose_count_is_not_its_programs_is_refused(monkeypatch, tmp_path):
    path = tmp_path / "floor.json"
    path.write_text(parity.dump_floor(parity.Floor(3, ["a"])), encoding="utf-8")
    monkeypatch.setattr(parity, "FLOOR", path)
    monkeypatch.setattr(parity, "entries", lambda only: [])
    monkeypatch.setattr(parity, "suite", lambda entries: [outcome("a")])
    with pytest.raises(SystemExit, match="counts 3 programs and names 1"):
        parity.main(["floor", "check"])

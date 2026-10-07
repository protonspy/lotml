"""The RunPod ledger and the cap a run is held to (specs/training-pipeline/ R1.2, R1.3)."""

from datetime import UTC, datetime
from decimal import Decimal
from pathlib import Path

import pytest

from lotml_harness.guide import ledger as ledgers
from lotml_harness.guide.ledger import CapExceeded, Ledger, estimate

START = datetime(2026, 10, 7, 10, 0, 0, tzinfo=UTC)


def opened(ledger: Ledger, pod: str, hourly: str = "0.34", hours: int = 3) -> None:
    ledger.open(
        run="r1", pod=pod, stages=["sft"], gpu="NVIDIA GeForce RTX 4090", cloud="COMMUNITY",
        hourly=Decimal(hourly), hours=hours, started=START,
    )  # fmt: skip


def test_the_estimate_is_the_hourly_price_times_the_deadline():
    assert estimate(Decimal("0.34"), 3) == Decimal("1.02")


def test_a_closed_pod_costs_its_minutes_rounded_up(tmp_path: Path):
    ledger = Ledger(tmp_path / "runpod.jsonl", None)
    opened(ledger, "pod_1")
    row = ledger.close("pod_1", datetime(2026, 10, 7, 10, 20, 30, tzinfo=UTC))
    assert row["minutes"] == 21
    assert Decimal(row["cost"]) == Decimal("0.1190")
    assert Ledger(tmp_path / "runpod.jsonl", None).spent() == Decimal("0.1190")


def test_an_open_pod_counts_at_its_full_deadline(tmp_path: Path):
    ledger = Ledger(tmp_path / "runpod.jsonl", None)
    opened(ledger, "pod_1")
    ledger.close("pod_1", datetime(2026, 10, 7, 11, 0, 0, tzinfo=UTC))
    opened(ledger, "pod_2", hourly="0.50", hours=2)
    assert ledger.spent() == Decimal("0.34") + Decimal("1.00")


def test_a_run_past_the_cap_is_refused_and_one_at_it_is_not(tmp_path: Path):
    ledger = Ledger(tmp_path / "runpod.jsonl", None)
    opened(ledger, "pod_1", hourly="1.00", hours=20)
    ledger.check(Decimal("25"), Decimal("5.00"))
    with pytest.raises(CapExceeded, match=r"20\.00 spent \+ 5\.01 estimated > 25\.00"):
        ledger.check(Decimal("25"), Decimal("5.01"))


def test_a_pod_is_closed_once_and_only_a_known_one(tmp_path: Path):
    ledger = Ledger(tmp_path / "runpod.jsonl", None)
    opened(ledger, "pod_1")
    ledger.close("pod_1", START)
    with pytest.raises(KeyError):
        ledger.close("pod_1", START)
    with pytest.raises(KeyError):
        ledger.close("pod_9", START)


def test_the_report_lists_every_pod_and_the_total_against_the_cap(tmp_path: Path):
    ledger = Ledger(tmp_path / "runpod.jsonl", None)
    opened(ledger, "pod_1")
    ledger.close("pod_1", datetime(2026, 10, 7, 10, 20, 30, tzinfo=UTC))
    opened(ledger, "pod_2")
    text = ledger.markdown(Decimal("25"))
    assert "| r1 | pod_1 | sft | NVIDIA GeForce RTX 4090 | COMMUNITY | 0.34 |" in text
    assert "| 21 | 0.12 |" in text
    assert "open, counted at its 3 h deadline: 1.02" in text
    assert "Spent 1.14 of 25.00 USD" in text


def test_the_spend_counts_the_committed_copy_s_rows_too(tmp_path: Path):
    home, committed = tmp_path / "home.jsonl", tmp_path / "committed.jsonl"
    elsewhere = Ledger(committed, None)
    opened(elsewhere, "pod_old", hourly="2.00", hours=1)
    elsewhere.close("pod_old", datetime(2026, 10, 7, 11, 0, 0, tzinfo=UTC))
    ledger = Ledger(home, committed)
    opened(ledger, "pod_1")
    ledger.close("pod_1", datetime(2026, 10, 7, 11, 0, 0, tzinfo=UTC))
    assert ledger.spent() == Decimal("2.00") + Decimal("0.34"), "a reset tree cannot lower it"
    ledger.commit()
    assert {r["pod"] for r in Ledger(committed, None).rows()} == {"pod_old", "pod_1"}


def test_a_change_waits_for_the_lock_and_gives_up(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    monkeypatch.setattr(ledgers, "LOCK_WAIT", 0.2)
    ledger = Ledger(tmp_path / "runpod.jsonl", None)
    (tmp_path / "runpod.lock").write_text("", encoding="utf-8")
    with pytest.raises(TimeoutError, match="is held"):
        opened(ledger, "pod_1")
    (tmp_path / "runpod.lock").unlink()
    opened(ledger, "pod_1")
    assert not (tmp_path / "runpod.lock").exists()

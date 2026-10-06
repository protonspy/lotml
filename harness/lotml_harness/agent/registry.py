"""The licence registry: for every task source, model and provider, whether training on what it
produced is permitted, with the evidence, the date it was checked and the notice it needs
(specs/trace-dataset/ R1.1-R1.5). Default deny: a run is exported only when its source, its model
and every provider that served it permit training, each entry complete.
"""

import datetime
import tomllib
from dataclasses import dataclass
from pathlib import Path

from lotml_harness import ROOT
from lotml_harness.agent.dataset import source_of

REGISTRY = Path(__file__).with_name("licences.toml")
SOURCE_FIELDS = {"licence", "training", "evidence", "checked", "notice"}
MODEL_FIELDS = {"terms", "training", "evidence", "checked", "notice", "providers"}
PROVIDER_FIELDS = {"training", "evidence", "checked", "notice"}


class RegistryError(ValueError):
    """A registry that cannot be read as written: the export stops and says why."""


def permitted(entry: dict) -> bool:
    """An entry permits training only when `training` is exactly `permitted`, its evidence is a
    repository path that exists or an `https://` URL, and it names the date checked and the
    notice."""
    evidence, checked, notice = entry.get("evidence"), entry.get("checked"), entry.get("notice")
    found = (
        isinstance(evidence, str)
        and bool(evidence)
        and (evidence.startswith("https://") or (ROOT / evidence).exists())
    )
    dated = isinstance(checked, datetime.date) or (isinstance(checked, str) and bool(checked))
    return (
        entry.get("training") == "permitted"
        and found
        and dated
        and isinstance(notice, str)
        and bool(notice)
    )


@dataclass
class Registry:
    sources: dict[str, dict]
    models: dict[str, dict]

    def permits_source(self, source: str) -> bool:
        return source in self.sources and permitted(self.sources[source])

    def decide(self, source: str, model: str, providers: list[str]) -> tuple[bool, str | None]:
        """Whether a run may be exported, and when not, the first reason why."""
        if source not in self.sources:
            return False, f"source {source} has no entry"
        if not permitted(self.sources[source]):
            return False, f"source {source} not permitted"
        if model not in self.models:
            return False, f"model {model} has no entry"
        if not permitted(self.models[model]):
            return False, f"model {model} not permitted"
        if not providers:
            return False, "no provider named"
        served = self.models[model].get("providers", {})
        for provider in providers:
            if provider not in served:
                return False, f"provider {provider} has no entry"
            if not permitted(served[provider]):
                return False, f"provider {provider} not permitted"
        return True, None


def identity(trace: dict, path: Path, root: Path) -> tuple[dict | None, str | None]:
    """A run's task, source, model, arm and providers, taken from the row inside its trace, its
    source derived again from the task id; None and the reason when the row names no source, or
    one the task id does not give, or the trace's path does not match the row (R1.6)."""
    row = trace.get("row") or {}
    task, model, arm = str(row.get("task")), str(row.get("model")), str(row.get("arm"))
    if "source" not in row:
        return None, "no source"
    if row["source"] != source_of(task):
        return None, "source disagrees with the task"
    expected = root / model.replace("/", "__") / arm / f"{task}-{row.get('attempt')}.json"
    if path.resolve() != expected.resolve():
        return None, "path disagrees with the row"
    providers = sorted(row.get("providers") or {})
    return {"task": task, "source": row["source"], "model": model, "arm": arm,
            "providers": providers}, None  # fmt: skip


def _fields(entry: object, allowed: set[str], where: str) -> dict:
    if not isinstance(entry, dict):
        raise RegistryError(f"{where} is not a table")
    unknown = set(entry) - allowed
    if unknown:
        raise RegistryError(f"{where} has unknown fields {sorted(unknown)}")
    return entry


def read(path: Path = REGISTRY) -> Registry:
    """The registry at `path`; RegistryError when it does not parse, or holds an unknown table, an
    unknown field or a duplicate entry (a duplicate is a TOML error)."""
    try:
        data = tomllib.loads(path.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as error:
        raise RegistryError(f"{path.name}: {error}") from error
    unknown = set(data) - {"sources", "models"}
    if unknown:
        raise RegistryError(f"{path.name} has unknown tables {sorted(unknown)}")
    sources = {
        name: _fields(entry, SOURCE_FIELDS, f"sources.{name}")
        for name, entry in data.get("sources", {}).items()
    }
    models = {}
    for name, entry in data.get("models", {}).items():
        models[name] = _fields(entry, MODEL_FIELDS, f"models.{name}")
        for provider, served in models[name].get("providers", {}).items():
            _fields(served, PROVIDER_FIELDS, f"models.{name}.providers.{provider}")
    return Registry(sources, models)

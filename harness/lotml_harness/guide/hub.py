"""The training pipeline's artifacts in a private Hugging Face repository: records by digest, and
under `runs/<run>/` each run's checkpoints, models, report and lineage (specs/training-pipeline/
R2.1, R2.2, R2.4).

The token is read from `HF_TOKEN` and handed to the client only; nothing is written to a
repository that is not private.
"""

import hashlib
import json
import os
import tempfile
from collections.abc import Callable
from pathlib import Path


class HubError(RuntimeError):
    """A repository the pipeline will not write to, or no token to reach it."""


def token() -> str:
    found = os.environ.get("HF_TOKEN")
    if not found:
        raise HubError("HF_TOKEN is not set: export a token that can write the repository")
    return found


def digest(records: Path) -> str:
    """The SHA-256 of a records directory's `.jsonl` files, read in name order — the digest the
    guide evaluation's guard computes, so the two name the same records the same way."""
    found = hashlib.sha256()
    for file in sorted(records.glob("*.jsonl")):
        found.update(file.read_bytes())
    return found.hexdigest()


class Hub:
    """One private model repository. `api` is an `HfApi`, and `download` is `snapshot_download`;
    both are passed in so the pipeline's logic is tested without the network."""

    def __init__(self, repo: str, api: object | None = None, download: Callable | None = None):
        if api is None or download is None:
            from huggingface_hub import HfApi, snapshot_download

            api, download = api or HfApi(), download or snapshot_download
        self.repo, self.api, self.download = repo, api, download
        self._private = False

    def ensure_private(self) -> None:
        """Create the repository private if it is absent; HubError if it exists and is public."""
        if self._private:
            return
        self.api.create_repo(self.repo, private=True, exist_ok=True, token=token())
        if not getattr(self.api.repo_info(self.repo, token=token()), "private", False):
            raise HubError(f"{self.repo} is not private: the pipeline writes nothing to it")
        self._private = True

    def put(self, local: Path, remote: str, message: str) -> None:
        """Upload a file or a directory to `remote`, after checking the repository is private."""
        self.ensure_private()
        if local.is_dir():
            self.api.upload_folder(
                folder_path=str(local), path_in_repo=remote, repo_id=self.repo,
                commit_message=message, token=token(),
            )  # fmt: skip
        else:
            self.api.upload_file(
                path_or_fileobj=str(local), path_in_repo=remote, repo_id=self.repo,
                commit_message=message, token=token(),
            )  # fmt: skip

    def files(self, remote: str) -> list[str]:
        """The repository's files at or under `remote`."""
        listed = self.api.list_repo_files(self.repo, token=token())
        return [f for f in listed if f == remote or f.startswith(remote.rstrip("/") + "/")]

    def get(self, remote: str, local: Path) -> Path:
        """Download `remote`, a file or a directory, into `local`; the local path it lands at."""
        self.download(
            repo_id=self.repo, allow_patterns=[remote, f"{remote.rstrip('/')}/**"],
            local_dir=str(local), token=token(),
        )  # fmt: skip
        return local / remote

    def put_records(self, records: Path) -> str:
        """Upload a records directory once, under `records/<digest>/`; its digest."""
        found = digest(records)
        if not self.files(f"records/{found}"):
            self.put(records, f"records/{found}", f"records {found[:12]}")
        return found

    def put_lineage(self, run: str, records: str, inputs: dict[str, str], commit: str) -> None:
        """Write `runs/<run>/run.json`: the records' digest, the commit, and per stage the run
        whose outputs it read, so a GGUF file's lineage reads from the repository alone."""
        lineage = {"run": run, "records": records, "commit": commit, "inputs": inputs}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "run.json"
            path.write_text(json.dumps(lineage, indent=2), encoding="utf-8")
            self.put(path, f"runs/{run}/run.json", f"run {run}: lineage")

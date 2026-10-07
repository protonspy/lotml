"""The VS Code extension: LotML declared for `.lot` and `.lotml` with the file icon, and the
language server started from `lotml.path` (specs/file-icons/ R3.1-R3.3)."""

import json
import re
import shutil
import subprocess

import pytest

from lotml_harness import ROOT, icons

EXTENSION = ROOT / "editors" / "vscode"


def manifest() -> dict:
    return json.loads((EXTENSION / "package.json").read_text(encoding="utf-8"))


def test_lotml_is_declared_for_both_extensions_with_the_file_icon():
    [language] = manifest()["contributes"]["languages"]
    assert language["id"] == "lotml" and "LotML" in language["aliases"]
    assert language["extensions"] == [".lot", ".lotml"]
    for theme in ("light", "dark"):
        icon = EXTENSION / language["icon"][theme]
        assert icon.read_bytes() == icons.images()[32], f"the {theme} icon is the file icon"


def test_the_language_comments_with_hash_pairs_brackets_and_indents_after_a_colon():
    [language] = manifest()["contributes"]["languages"]
    configuration = json.loads((EXTENSION / language["configuration"]).read_text("utf-8"))
    assert configuration["comments"] == {"lineComment": "#"}
    assert configuration["brackets"] == [["(", ")"], ["[", "]"], ["{", "}"]]
    [rule] = configuration["onEnterRules"]
    assert rule["action"] == {"indent": "indent"}
    opens = re.compile(rule["beforeText"])
    for line in ("fn f() -> int:", "    if x > 0:  # positive", 'test "median":'):
        assert opens.search(line), line
    for line in ("x = 1", "return {a: 1}", "label: str = name"):
        assert not opens.search(line), line


def test_the_server_is_lotml_path_and_lotml_by_default():
    setting = manifest()["contributes"]["configuration"]["properties"]["lotml.path"]
    assert (setting["type"], setting["default"]) == ("string", "lotml")


def test_a_workspace_can_neither_choose_the_compiler_nor_run_it_untrusted():
    setting = manifest()["contributes"]["configuration"]["properties"]["lotml.path"]
    assert setting["scope"] == "machine", "read from user settings only (R3.4)"
    assert manifest()["capabilities"]["untrustedWorkspaces"]["supported"] is False


@pytest.mark.skipif(shutil.which("node") is None, reason="Node runs the extension's own tests")
def test_the_extension_starts_the_server_and_says_once_when_it_cannot():
    node = shutil.which("node")
    done = subprocess.run(  # noqa: S603
        [node, "--test", "test/extension.test.js"],
        cwd=EXTENSION,
        capture_output=True,
        text=True,
        check=False,
    )
    assert done.returncode == 0, done.stdout + done.stderr

#!/usr/bin/env bash
# What a release archive does on a machine with no Python (plans/python-via-uv.md 3.2): given the
# directory an archive unpacked into, run `lotml run`, `lotml test` and `lotml bind textwrap` with
# nothing on the path and no CPython uv has installed, so CPython 3.14 is downloaded through the
# uv beside lotml, said first; then, with the offline switch and again no CPython, check that the
# run fails naming what is missing and downloads nothing.
#
#     release/smoke.sh <unpacked directory> [online|offline|both]
set -euo pipefail

unpacked=$1
part=${2:-both}
lotml="$unpacked/lotml"
if [ -f "$lotml.exe" ]; then lotml="$lotml.exe"; fi
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cd "$work"
printf 'fn main():\n    print("ran")\n' > hello.lotml
printf 'test "adds":\n    assert 1 + 1 == 2\n' > adds.lotml

# Nothing on the path: lotml finds uv beside itself and runs CPython by absolute path. Windows
# keeps its system directory, where every program's own libraries come from.
bare=/nonexistent
if [ -n "${SYSTEMROOT:-}" ]; then bare="$(cygpath -u "$SYSTEMROOT")/System32"; fi
# A path given to lotml in a variable, as the system writes one: Git Bash converts the arguments
# of a Windows program, never its environment.
native() { if command -v cygpath > /dev/null; then cygpath -w "$1"; else printf '%s' "$1"; fi; }
cache=$(native "$work/cache")
run() { env -u VIRTUAL_ENV -u LOTML_PYTHON -u LOTML_OFFLINE PATH="$bare" LOTML_CACHE_DIR="$cache" "$@"; }

if [ "$part" != offline ]; then
  mkdir "$work/smoke-pythons"
  pythons=$(native "$work/smoke-pythons")
  said=$(run UV_PYTHON_INSTALL_DIR="$pythons" "$lotml" run hello.lotml 2>&1)
  echo "$said"
  grep -q "downloading CPython 3.14" <<< "$said" || { echo "::error::lotml run did not say it downloads CPython 3.14"; exit 1; }
  [ "$(tail -n 1 <<< "$said" | tr -d '\r')" = ran ] || { echo "::error::lotml run did not run the program"; exit 1; }
  # The CPython it ran on is the one uv just installed, not one the machine had.
  recorded=$(run UV_PYTHON_INSTALL_DIR="$pythons" "$lotml" run --json hello.lotml 2>&1 | tail -n 1)
  echo "$recorded"
  grep -q "smoke-pythons" <<< "$recorded" || { echo "::error::lotml ran a CPython other than the one uv installed"; exit 1; }
  run UV_PYTHON_INSTALL_DIR="$pythons" "$lotml" test adds.lotml
  again=$(run UV_PYTHON_INSTALL_DIR="$pythons" "$lotml" run hello.lotml 2>&1)
  if grep -q "downloading" <<< "$again"; then echo "::error::a second run downloaded again"; exit 1; fi
  [ "$(tail -n 1 <<< "$again" | tr -d '\r')" = ran ] || { echo "::error::the second run did not run the program"; exit 1; }
  run "$lotml" bind textwrap --out bindings
  grep -q "^fn dedent(" bindings/py.textwrap.lotmli || { echo "::error::bind wrote no dedent"; exit 1; }
fi

if [ "$part" != online ]; then
  mkdir "$work/none"
  none=$(native "$work/none")
  status=0
  said=$(run UV_PYTHON_INSTALL_DIR="$none" LOTML_OFFLINE=1 "$lotml" run hello.lotml 2>&1) || status=$?
  echo "$said"
  [ "$status" != 0 ] || { echo "::error::lotml run offline with no CPython succeeded"; exit 1; }
  grep -q "CPython 3.14 is not installed through uv" <<< "$said" || { echo "::error::the failure did not name what is missing"; exit 1; }
  [ -z "$(ls -A "$work/none")" ] || { echo "::error::lotml downloaded offline"; exit 1; }
fi
echo "smoke: $part passed"

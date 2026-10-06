"""Run a program under a memory cap, its children with it:

    python -m lotml_harness.confine <bytes> <program> <arguments>...

The cap is set on this process first — an address-space rlimit on POSIX, a job object on Windows
— and the program inherits it; on Windows the job ends every process in it when this one dies, so
killing this process at a deadline kills the program it started.
"""

import subprocess
import sys

from lotml_harness.limits import limit_memory


def main() -> None:
    limit_memory(int(sys.argv[1]))
    sys.exit(subprocess.run(sys.argv[2:], check=False).returncode)  # noqa: S603


if __name__ == "__main__":
    main()

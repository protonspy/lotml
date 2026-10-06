"""A cap on a process's memory, with nothing else imported: the confining wrapper starts fast."""

import sys
from typing import ClassVar


def limit_memory(limit: int) -> None:
    """Cap this process's memory: an address-space rlimit on POSIX, a job object on Windows."""
    if sys.platform != "win32":
        import resource

        resource.setrlimit(resource.RLIMIT_AS, (limit, limit))
        return
    import ctypes
    from ctypes import wintypes

    class IoCounters(ctypes.Structure):
        _fields_: ClassVar = [(f"c{i}", ctypes.c_ulonglong) for i in range(6)]

    class BasicLimits(ctypes.Structure):
        _fields_: ClassVar = [
            ("PerProcessUserTimeLimit", ctypes.c_int64),
            ("PerJobUserTimeLimit", ctypes.c_int64),
            ("LimitFlags", wintypes.DWORD),
            ("MinimumWorkingSetSize", ctypes.c_size_t),
            ("MaximumWorkingSetSize", ctypes.c_size_t),
            ("ActiveProcessLimit", wintypes.DWORD),
            ("Affinity", ctypes.c_size_t),
            ("PriorityClass", wintypes.DWORD),
            ("SchedulingClass", wintypes.DWORD),
        ]

    class ExtendedLimits(ctypes.Structure):
        _fields_: ClassVar = [
            ("BasicLimitInformation", BasicLimits),
            ("IoInfo", IoCounters),
            ("ProcessMemoryLimit", ctypes.c_size_t),
            ("JobMemoryLimit", ctypes.c_size_t),
            ("PeakProcessMemoryUsed", ctypes.c_size_t),
            ("PeakJobMemoryUsed", ctypes.c_size_t),
        ]

    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel32.CreateJobObjectW.restype = wintypes.HANDLE
    kernel32.GetCurrentProcess.restype = wintypes.HANDLE
    kernel32.SetInformationJobObject.argtypes = [
        wintypes.HANDLE,
        ctypes.c_int,
        ctypes.c_void_p,
        wintypes.DWORD,
    ]
    kernel32.AssignProcessToJobObject.argtypes = [wintypes.HANDLE, wintypes.HANDLE]
    job = kernel32.CreateJobObjectW(None, None)
    limits = ExtendedLimits()
    # JOB_OBJECT_LIMIT_PROCESS_MEMORY, and JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: a process the
    # program starts dies with the child instead of outliving the wall clock.
    limits.BasicLimitInformation.LimitFlags = 0x100 | 0x2000
    limits.ProcessMemoryLimit = limit
    extended = 9  # JobObjectExtendedLimitInformation
    if not (
        job
        and kernel32.SetInformationJobObject(
            job, extended, ctypes.byref(limits), ctypes.sizeof(limits)
        )
        and kernel32.AssignProcessToJobObject(job, kernel32.GetCurrentProcess())
    ):
        raise OSError(ctypes.get_last_error(), "could not limit the child's memory")

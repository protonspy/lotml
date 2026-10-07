"""RunPod's REST API v2 through the standard library: a GPU's hourly price from the catalog, and a
pod created, read and terminated (specs/training-pipeline/ R1.1, R1.4).

The key is read from `RUNPOD_API_KEY` at each call and goes only into the request's header; an
error carries RunPod's problem — status, title and detail — and never the request.
"""

import json
import os
import time
import urllib.error
import urllib.parse
import urllib.request
from collections.abc import Callable

API = "https://api.runpod.io/v2"
TIMEOUT = 60
"""Seconds one request may take."""
GONE = {"TERMINATED"}
"""Statuses a pod that is not billing reports, besides answering 404."""


class _NoRedirect(urllib.request.HTTPRedirectHandler):
    """Follow no redirect: urllib would carry the Authorization header to wherever it points."""

    def redirect_request(self, *_: object) -> None:
        return None


urlopen = urllib.request.build_opener(_NoRedirect).open
"""The opener every request goes through; a redirect comes back as the HTTP error it is."""


class RunPodError(RuntimeError):
    """A refusal from RunPod, or a pod that would not go away."""


class Transient(RunPodError):
    """RunPod could not be reached: the network or a timeout, worth asking again."""


ATTEMPTS = 3
"""Times a read or a termination is tried before a transient failure is raised."""


def key() -> str:
    found = os.environ.get("RUNPOD_API_KEY")
    if not found:
        raise RunPodError("RUNPOD_API_KEY is not set: export it to run on RunPod")
    return found


def call(method: str, path: str, body: dict | None = None) -> dict | None:
    """One request to the API; its JSON, or None for an empty body. RunPodError on any status but
    2xx, with the problem's status, title and detail."""
    request = urllib.request.Request(  # noqa: S310 - a fixed https URL
        API + path,
        data=None if body is None else json.dumps(body).encode(),
        method=method,
        headers={
            "Authorization": f"Bearer {key()}",
            "Content-Type": "application/json",
            "Accept": "application/json",
        },
    )
    try:
        with urlopen(request, timeout=TIMEOUT) as response:
            data = response.read()
    except urllib.error.HTTPError as error:
        raise RunPodError(_problem(error.code, error.read())) from None
    except OSError as error:
        raise Transient(f"RunPod could not be reached: {type(error).__name__}") from None
    return json.loads(data) if data.strip() else None


def retried(
    action: Callable[[], object], sleep: Callable[[float], None] = time.sleep, wait: float = 5
) -> object:
    """`action`, asked again after a transient failure, up to `ATTEMPTS` times."""
    for attempt in range(ATTEMPTS):
        try:
            return action()
        except Transient:
            if attempt == ATTEMPTS - 1:
                raise
            sleep(wait)
    raise AssertionError("unreachable")


def _problem(status: int, data: bytes) -> str:
    try:
        found = json.loads(data)
    except json.JSONDecodeError:
        found = {}
    title = found.get("title") if isinstance(found, dict) else None
    detail = found.get("detail") if isinstance(found, dict) else None
    return f"RunPod answered {status}" + "".join(f": {part}" for part in (title, detail) if part)


def price(gpu: str, cloud: str = "COMMUNITY") -> float:
    """The GPU's hourly price in USD on `cloud`, `COMMUNITY` or `SECURE`, from the catalog."""
    found = call("GET", f"/catalog/gpus/{urllib.parse.quote(gpu, safe='')}") or {}
    hourly = (found.get("price") or {}).get(cloud.lower())
    if not isinstance(hourly, int | float) or hourly <= 0:
        raise RunPodError(f"{gpu} has no {cloud.lower()} price in the catalog")
    return float(hourly)


def pod_request(
    name: str, image: str, gpu: str, cloud: str, disk: int, env: dict[str, str], command: str
) -> dict:
    """The body that creates a one-GPU pod running `command` under bash, with no ports and no
    volume: what must outlive the pod goes to the Hugging Face repository."""
    return {
        "name": name,
        "image": image,
        "gpu": {"id": gpu, "count": 1},
        "cloud": cloud,
        "disk": disk,
        "env": env,
        "entrypoint": ["bash", "-c"],
        "cmd": [command],
    }


def create(request: dict) -> dict:
    found = call("POST", "/pods", request)
    if not isinstance(found, dict) or not found.get("id"):
        raise RunPodError("RunPod created no pod")
    return found


def pods() -> list[dict]:
    """Every pod of the account, as RunPod lists them."""
    found = retried(lambda: call("GET", "/pods"))
    listed = found.get("pods", []) if isinstance(found, dict) else found or []
    return [p for p in listed if isinstance(p, dict) and p.get("id")]


def named(name: str) -> str | None:
    """The id of the pod called `name`, for a pod RunPod created whose id never came back."""
    return next((p["id"] for p in pods() if p.get("name") == name), None)


def status(pod: str, sleep: Callable[[float], None] = time.sleep) -> dict | None:
    """The pod as RunPod reports it, or None when it is gone: a 404, or a terminated status. A
    transient failure is asked again before it is raised."""
    try:
        found = retried(lambda: call("GET", f"/pods/{urllib.parse.quote(pod, safe='')}"), sleep)
    except RunPodError as error:
        if str(error).startswith("RunPod answered 404"):
            return None
        raise
    if not isinstance(found, dict):
        raise RunPodError(f"RunPod described no pod {pod}")
    return None if found.get("status") in GONE else found


def terminate(
    pod: str, wait: float = 300, poll: float = 5, sleep: Callable[[float], None] = time.sleep
) -> None:
    """Terminate the pod and return once RunPod no longer reports it; RunPodError if it still does
    after `wait` seconds, so a pod is never left billing in silence. The termination is always
    asked for — a pod one look found gone is confirmed gone by a second — and a 404 to it means
    the pod is already gone."""
    path = f"/pods/{urllib.parse.quote(pod, safe='')}/action"
    try:
        retried(lambda: call("POST", path, {"action": "terminate"}), sleep)
    except RunPodError as error:
        if not str(error).startswith("RunPod answered 404"):
            raise
    waited = 0.0
    while status(pod, sleep) is not None:
        if waited >= wait:
            raise RunPodError(f"pod {pod} still exists {wait:.0f} s after it was terminated")
        sleep(poll)
        waited += poll

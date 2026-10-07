"""RunPod's REST API: price, create, status and terminate, against a fake API
(specs/training-pipeline/ R1.1, R1.4)."""

import io
import json
import urllib.error

import pytest

from lotml_harness.guide import runpod
from lotml_harness.guide.runpod import RunPodError

KEY = "rp_test_0123456789abcdef"


class Api:
    """Answers each request from `replies`, in order, and keeps what it was sent."""

    def __init__(self, *replies: tuple[int, object]):
        self.replies = list(replies)
        self.sent: list[tuple[str, str, dict | None, dict]] = []

    def __call__(self, request, timeout: float):
        body = json.loads(request.data) if request.data else None
        self.sent.append(
            (request.get_method(), request.full_url, body, dict(request.header_items()))
        )
        status, reply = self.replies.pop(0)
        data = json.dumps(reply).encode() if reply is not None else b""
        if status >= 400:
            raise urllib.error.HTTPError(request.full_url, status, "error", {}, io.BytesIO(data))
        return io.BytesIO(data)


@pytest.fixture
def api(monkeypatch: pytest.MonkeyPatch):
    monkeypatch.setenv("RUNPOD_API_KEY", KEY)

    def install(*replies: tuple[int, object]) -> Api:
        fake = Api(*replies)
        monkeypatch.setattr(runpod.urllib.request, "urlopen", fake)
        return fake

    return install


def test_the_price_comes_from_the_catalog_for_the_cloud_asked(api):
    fake = api(
        (200, {"id": "NVIDIA GeForce RTX 4090", "price": {"secure": 0.69, "community": 0.34}})
    )
    assert runpod.price("NVIDIA GeForce RTX 4090") == 0.34
    method, url, _, headers = fake.sent[0]
    assert (method, url) == ("GET", f"{runpod.API}/catalog/gpus/NVIDIA%20GeForce%20RTX%204090")
    assert headers["Authorization"] == f"Bearer {KEY}"
    api((200, {"price": {"secure": 0.69, "community": None}}))
    with pytest.raises(RunPodError, match="no community price"):
        runpod.price("NVIDIA GeForce RTX 4090")


def test_a_pod_is_created_with_one_gpu_bash_and_no_ports(api):
    fake = api((201, {"id": "pod_1", "status": "PROVISIONING", "cost": 0.34}))
    request = runpod.pod_request(
        "guide-r1",
        "runpod/pytorch:x",
        "NVIDIA GeForce RTX 4090",
        "COMMUNITY",
        40,
        {"RUN": "r1"},
        "echo hi",
    )
    assert runpod.create(request)["id"] == "pod_1"
    _, url, body, _ = fake.sent[0]
    assert url == f"{runpod.API}/pods"
    assert body["gpu"] == {"id": "NVIDIA GeForce RTX 4090", "count": 1}
    assert (body["entrypoint"], body["cmd"]) == (["bash", "-c"], ["echo hi"])
    assert "ports" not in body and "mounts" not in body


def test_a_pod_that_answers_404_or_terminated_is_gone(api):
    api((404, {"title": "Not Found", "status": 404, "detail": "pod not found"}))
    assert runpod.status("pod_1") is None
    api((200, {"id": "pod_1", "status": "TERMINATED"}))
    assert runpod.status("pod_1") is None
    api((200, {"id": "pod_1", "status": "RUNNING"}))
    assert runpod.status("pod_1")["status"] == "RUNNING"


def test_terminate_waits_until_the_pod_is_gone(api):
    fake = api(
        (200, {"id": "pod_1", "status": "RUNNING"}),
        (200, None),
        (200, {"id": "pod_1", "status": "RUNNING"}),
        (404, {"title": "Not Found"}),
    )
    slept = []
    runpod.terminate("pod_1", wait=60, poll=5, sleep=slept.append)
    assert fake.sent[1][:3] == ("POST", f"{runpod.API}/pods/pod_1/action", {"action": "terminate"})
    assert slept == [5]


def test_a_pod_that_will_not_go_is_an_error(api):
    running = (200, {"id": "pod_1", "status": "RUNNING"})
    api(running, (200, None), *[running] * 5)
    with pytest.raises(RunPodError, match="still exists"):
        runpod.terminate("pod_1", wait=10, poll=5, sleep=lambda _: None)


def test_an_empty_description_is_not_taken_for_a_pod_gone(api):
    api((200, None))
    with pytest.raises(RunPodError, match="described no pod"):
        runpod.status("pod_1")


def test_an_error_carries_the_problem_and_never_the_key(api):
    api((403, {"title": "Forbidden", "status": 403, "detail": "access denied"}))
    with pytest.raises(RunPodError) as raised:
        runpod.create({"name": "x"})
    assert str(raised.value) == "RunPod answered 403: Forbidden: access denied"
    assert KEY not in repr(raised.value) and raised.value.__cause__ is None


def test_no_key_no_request(monkeypatch: pytest.MonkeyPatch):
    monkeypatch.delenv("RUNPOD_API_KEY", raising=False)
    monkeypatch.setattr(runpod.urllib.request, "urlopen", lambda *_: pytest.fail("no request"))
    with pytest.raises(RunPodError, match="RUNPOD_API_KEY is not set"):
        runpod.price("NVIDIA GeForce RTX 4090")


def test_a_network_failure_is_transient_and_a_read_is_asked_again(api, monkeypatch):
    fake = api((200, {"id": "pod_1", "status": "RUNNING"}))
    real = fake.__call__
    failures = iter([urllib.error.URLError("reset"), None])

    def flaky(request, timeout):
        failure = next(failures)
        if failure is not None:
            raise failure
        return real(request, timeout)

    monkeypatch.setattr(runpod.urllib.request, "urlopen", flaky)
    slept = []
    assert runpod.status("pod_1", sleep=slept.append)["status"] == "RUNNING"
    assert slept == [5]
    monkeypatch.setattr(
        runpod.urllib.request, "urlopen", lambda *_, **__: (_ for _ in ()).throw(TimeoutError())
    )
    with pytest.raises(runpod.Transient, match="could not be reached: TimeoutError"):
        runpod.status("pod_1", sleep=lambda _: None)


def test_a_pod_is_found_by_its_name(api):
    api(
        (
            200,
            {"pods": [{"id": "pod_1", "name": "other"}, {"id": "pod_2", "name": "lotml-guide-r1"}]},
        )
    )
    assert runpod.named("lotml-guide-r1") == "pod_2"
    api((200, {"pods": []}))
    assert runpod.named("lotml-guide-r1") is None

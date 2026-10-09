"""Deterministic harness-process custody; no child, Docker or product execution."""
import io
from pathlib import Path
import subprocess
import sys
import tempfile
from unittest import mock
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from r7 import runner


class Host:
    def __init__(self, *, exited=None, wait_error=None):
        self.pid = 424242
        self.stdin = io.BytesIO()
        self.stdout = io.BytesIO()
        self.exited = exited
        self.wait_error = wait_error
        self.waits = []
        self.polls = 0

    def poll(self):
        self.polls += 1
        return self.exited

    def wait(self, *, timeout):
        self.waits.append(timeout)
        if self.wait_error is not None:
            raise self.wait_error
        self.exited = -9 if self.exited is None else self.exited
        return self.exited


class Selector:
    def __init__(self, failure=None):
        self.failure = failure
        self.registered = []
        self.closed = False

    def register(self, stream, events):
        self.registered.append((stream, events))
        if self.failure is not None:
            raise self.failure

    def close(self):
        self.closed = True


class EventCustody(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="layerfs-r7-host-custody-")
        self.folder = Path(self.directory.name)
        self.argv = ["synthetic-not-executed", "serve", "--receipt", str(self.folder / "runtime.events.jsonl")]

    def tearDown(self):
        self.directory.cleanup()

    def construct_failure(self, host, selector, original, *, writer=None, selector_factory=None):
        killed = []
        with mock.patch.object(runner.subprocess, "Popen", return_value=host) as launched, \
             mock.patch.object(runner.selectors, "DefaultSelector", selector_factory or mock.Mock(return_value=selector)), \
             mock.patch.object(runner.os, "killpg", side_effect=lambda pid, signal: killed.append((pid, signal))):
            patch = mock.patch.object(runner, "write_new", side_effect=writer) if writer else mock.patch.object(runner, "write_new", wraps=runner.write_new)
            with patch:
                with self.assertRaises(type(original)) as caught:
                    runner.EventProcess(self.argv, self.folder, "runtime")
            self.assertIs(caught.exception, original)
        self.assertEqual(launched.call_count, 1)
        self.assertTrue(launched.call_args.kwargs["start_new_session"])
        self.assertEqual(launched.call_args.kwargs["bufsize"], 0)
        self.assertEqual(killed, [(host.pid, 9)])
        self.assertEqual(host.waits, [1])
        owner = original.event_process
        self.assertIs(owner.process, host)
        self.assertIs(owner._retained_cause, original)
        self.assertIs(original.event_process_retention, owner._host_fence_result)
        self.assertIsNone(owner.container)
        self.assertEqual(owner.exec_ids, [])
        self.assertEqual(owner.child_event_receipts, [str(self.folder / "runtime.events.jsonl")])
        self.assertTrue(host.stdin.closed and host.stdout.closed)
        self.assertTrue(owner.stderr.closed and owner.raw.closed)
        return owner

    def test_selector_registration_failure_retains_partial_owner_and_fences_once(self):
        original = OSError("original selector registration refusal")
        selector, host = Selector(original), Host()
        owner = self.construct_failure(host, selector, original)
        self.assertEqual(original.original_phase, "event_process_selector_register")
        self.assertTrue(selector.closed)
        with mock.patch.object(runner.os, "killpg") as signal, mock.patch.object(runner, "write_new") as output:
            self.assertIs(owner.retain(original), owner._host_fence_result)
            later = ValueError("later wrapper is not the original failure")
            owner.retain(later)
            self.assertIs(later.event_process_original_cause, original)
        signal.assert_not_called()
        output.assert_not_called()
        self.assertEqual(host.waits, [1])

    def test_selector_factory_failure_after_launch_still_fences_known_child(self):
        original = OSError("original selector construction refusal")
        host = Host()
        owner = self.construct_failure(host, None, original, selector_factory=mock.Mock(side_effect=original))
        self.assertIsNone(owner.selector)
        self.assertEqual(original.original_phase, "event_process_selector_create")

    def test_custody_output_failure_does_not_escape_or_mask_launched_owner(self):
        original = OSError("original host custody output refusal")
        secondary = OSError("independent retained output refusal")
        def writer(path, value):
            if path.name == "runtime.host-custody.json":
                raise original
            raise secondary
        owner = self.construct_failure(Host(), Selector(), original, writer=writer)
        self.assertEqual(original.original_phase, "event_process_host_custody_output")
        errors = original.independent_custody_failures
        self.assertTrue(any(item["error"] is secondary and item["phase"] == "retained_custody_output" for item in errors))
        self.assertTrue(any(item["error"] is secondary and item["phase"] == "host_fence_output" for item in errors))
        self.assertEqual(owner._host_fence_result["host_signal"], "ORIGINAL_SIGKILL_SENT")

    def test_prelaunch_output_failure_creates_no_child_and_never_signals(self):
        (self.folder / "runtime.stderr").write_bytes(b"original retained output")
        with mock.patch.object(runner.subprocess, "Popen") as launched, mock.patch.object(runner.os, "killpg") as signal:
            with self.assertRaises(FileExistsError) as caught:
                runner.EventProcess(self.argv, self.folder, "runtime")
        launched.assert_not_called()
        signal.assert_not_called()
        self.assertIsNone(caught.exception.event_process.process)
        self.assertEqual(caught.exception.event_process_retention["host_owner"], "NO_ACKNOWLEDGED_CHILD")
        self.assertEqual((self.folder / "runtime.stderr").read_bytes(), b"original retained output")

    def test_launch_refusal_preserves_first_cause_without_guessing_pid(self):
        original = OSError("original Popen refusal")
        with mock.patch.object(runner.subprocess, "Popen", side_effect=original) as launched, mock.patch.object(runner.os, "killpg") as signal:
            with self.assertRaises(OSError) as caught:
                runner.EventProcess(self.argv, self.folder, "runtime")
        self.assertIs(caught.exception, original)
        self.assertEqual(launched.call_count, 1)
        signal.assert_not_called()
        self.assertIsNone(original.event_process_retention["host_pid"])
        self.assertEqual(original.original_phase, "event_process_host_launch")

    def normal_owner(self, host):
        selector = Selector()
        with mock.patch.object(runner.subprocess, "Popen", return_value=host), \
             mock.patch.object(runner.selectors, "DefaultSelector", return_value=selector):
            owner = runner.EventProcess(self.argv, self.folder, "runtime")
        self.addCleanup(owner.stderr.close)
        self.addCleanup(owner.raw.close)
        self.addCleanup(selector.close)
        return owner

    def test_normal_retain_preserves_exact_primary_and_failed_receipt_then_fences(self):
        host = Host()
        owner = self.normal_owner(host)
        original, output_error = ValueError("original event failure"), OSError("independent receipt disk refusal")
        original.original_phase = "original_event_delivery"
        with mock.patch.object(runner, "write_new", side_effect=output_error), mock.patch.object(runner.os, "killpg") as killed:
            result = owner.retain(original)
        self.assertEqual(killed.call_args_list, [mock.call(host.pid, 9)])
        self.assertEqual(host.waits, [1])
        self.assertEqual(result["cause"], "original event failure")
        self.assertEqual(result["original_phase"], "original_event_delivery")
        self.assertIs(original.event_process, owner)
        self.assertTrue(all(item["error"] is output_error for item in original.independent_custody_failures))
        self.assertEqual(result["container_disposition"], "NO_ACTION_OR_INFERENCE; original child evidence retained")

    def test_bounded_wait_refusal_retains_exact_failure_without_second_signal(self):
        waiting = subprocess.TimeoutExpired(self.argv, 1)
        host = Host(wait_error=waiting)
        owner = self.normal_owner(host)
        original = ValueError("original root event failure")
        with mock.patch.object(runner.os, "killpg") as killed:
            result = owner.retain(original)
            owner.retain(original)
        self.assertEqual(killed.call_count, 1)
        self.assertEqual(host.waits, [1])
        self.assertTrue(any(item["error"] is waiting for item in original.independent_custody_failures))
        self.assertEqual(result["observed_host_exit"], "UNAVAILABLE after bounded original wait")

    def test_already_exited_original_child_is_not_signalled(self):
        host = Host(exited=7)
        owner = self.normal_owner(host)
        original = ValueError("original child exit")
        with mock.patch.object(runner.os, "killpg") as signal:
            result = owner.retain(original)
        signal.assert_not_called()
        self.assertEqual(host.waits, [1])
        self.assertEqual(result["observed_host_exit"], 7)
        self.assertEqual(result["host_signal"], "NOT_ATTEMPTED")

    def test_short_control_write_never_resends_or_reads_a_guessed_reply(self):
        owner = runner.EventProcess.__new__(runner.EventProcess)
        owner.process = mock.Mock()
        owner.process.stdin.write.return_value = 2
        owner.event = mock.Mock()
        with mock.patch.object(runner.time, "monotonic", return_value=1):
            with self.assertRaisesRegex(runner.OriginalFailure, "outcome uncertain") as caught:
                owner.send("mount", "a" * 64, deadline=10)
        self.assertEqual(caught.exception.original_phase, "event_process_original_control_write")
        owner.process.stdin.write.assert_called_once()
        owner.process.stdin.flush.assert_not_called()
        owner.event.assert_not_called()

    def test_short_evidence_write_keeps_received_window_and_never_flushes(self):
        owner = runner.EventProcess.__new__(runner.EventProcess)
        owner.buffer = b""
        owner.selector, owner.process, owner.raw = mock.Mock(), mock.Mock(), mock.Mock()
        owner.selector.select.return_value = [object()]
        owner.raw.write.return_value = 2
        data = b'{"event":"original_acknowledgement"}\n'
        with mock.patch.object(runner.time, "monotonic", return_value=1), \
                mock.patch.object(runner.os, "read", return_value=data) as received:
            with self.assertRaisesRegex(runner.OriginalFailure, "no resend") as caught:
                owner.event("protocol_ready", 10)
        self.assertEqual(caught.exception.original_phase, "event_process_raw_write")
        self.assertEqual(owner.buffer, data)
        received.assert_called_once()
        owner.raw.write.assert_called_once_with(data)
        owner.raw.flush.assert_not_called()


if __name__ == "__main__":
    unittest.main()

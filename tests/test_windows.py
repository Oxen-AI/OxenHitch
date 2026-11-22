"""Verify Windows install discovery, policy precedence, and process identity.

| Stage | Mechanism | Tag | Ref |
| --- | --- | --- | --- |
| FIXTURE | Supply temporary install layouts and synthetic processes | [contract] | R4 |
| POLICY | Exercise machine/user inference precedence | [mandatory] | R4 |
| PROCESS | Exclude helpers and unrelated Claude executables | [mandatory] | R4 |

EARS guards: R4 WHEN a process name matches but its executable path does not, exclude it.
CLOSED: tests never open a Claude process or registry hive.
Preconditions: pytest supplies isolated temporary paths.
Failure modes: assertion failures identify Windows contract drift.
"""

from __future__ import annotations

from pathlib import Path

from hitch.windows import ClaudeInstall, inference_managed


class FakeProcess:
    """Expose the process fields read by exact-path matching.

    Require: executable is a full path and create_time is numeric.
    Guarantee: process methods return only injected fixture data.
    Maintain: process state is immutable during the test.
    Assert: PID remains unique within one fixture.
    """

    def __init__(self, pid: int, executable: Path, created: float = 123.0):
        """Create one synthetic process observation.

        Require: pid is positive.
        Guarantee: no operating-system process is queried.
        Maintain: executable and timestamp remain fixed.
        Assert: fixture values retain their input types.
        """
        self.pid = pid
        self._executable = str(executable)
        self._created = created

    def exe(self) -> str:
        """Return the injected executable path.

        Require: instance is initialized.
        Guarantee: returns the exact synthetic path.
        Maintain: no process state changes.
        Assert: path is not reduced to a basename.
        """
        return self._executable

    def create_time(self) -> float:
        """Return the injected creation time.

        Require: instance is initialized.
        Guarantee: returns the original numeric timestamp.
        Maintain: no process state changes.
        Assert: process identity includes time for future action checks.
        """
        return self._created


def test_install_discovery_includes_only_existing_main_executables(tmp_path: Path) -> None:
    """Discover the stable launcher and versioned main application paths.

    Require: temporary root represents AnthropicClaude.
    Guarantee: helpers and nonexistent version paths are excluded.
    Maintain: no system Claude directory is inspected.
    Assert: both supported launch and process paths are represented.
    """
    root = tmp_path / "AnthropicClaude"
    versioned = root / "app-6.200.1"
    (versioned / "resources").mkdir(parents=True)
    (root / "claude.exe").touch()
    (versioned / "claude.exe").touch()
    (versioned / "resources" / "helper.exe").touch()
    install = ClaudeInstall(root)
    assert set(install.executables()) == {root / "claude.exe", versioned / "claude.exe"}
    assert install.launch_executable == root / "claude.exe"


def test_process_matching_uses_full_path_and_excludes_cli_and_helper(tmp_path: Path) -> None:
    """Match only stable or versioned Claude Desktop executable paths.

    Require: temporary install contains a stable and versioned executable.
    Guarantee: same-name CLI and nested helper are excluded.
    Maintain: match includes PID and creation timestamp.
    Assert: exact candidates produce the only results.
    """
    root = tmp_path / "AnthropicClaude"
    versioned = root / "app-6.200.1"
    versioned.mkdir(parents=True)
    stable_exe = root / "claude.exe"
    app_exe = versioned / "claude.exe"
    helper = versioned / "resources" / "claude.exe"
    stable_exe.touch()
    app_exe.touch()
    install = ClaudeInstall(root)
    processes = [FakeProcess(10, stable_exe), FakeProcess(11, app_exe, 456.0), FakeProcess(12, helper), FakeProcess(13, tmp_path / "cli" / "claude.exe")]
    assert install.running_processes(processes) == [(10, 123.0), (11, 456.0)]


def test_machine_policy_replaces_user_policy_and_only_inference_marks_managed() -> None:
    """Verify hive precedence and relevant policy keys across varied inputs.

    Require: policy mappings are decoded registry values or None.
    Guarantee: machine key suppresses user-policy merging.
    Maintain: unrelated Claude settings do not signal managed inference.
    Assert: each independent policy case has an explicit expected result.
    """
    assert inference_managed(None, None) is False
    assert inference_managed(None, {"inferenceProvider": "gateway"}) is True
    assert inference_managed({"appBehavior": "managed"}, {"inferenceProvider": "gateway"}) is False
    assert inference_managed({"bootstrapUrl": "https://managed.example"}, None) is True

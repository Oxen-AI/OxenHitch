"""Focused parity tests for Python config, state, and model contracts.

| Stage | Mechanism | Tag | Ref |
| --- | --- | --- | --- |
| FIXTURE | Build isolated Claude roots | [contract] | R1 |
| VERIFY | Check profile preservation and restore | [mandatory] | R1 |
| VERIFY | Check state/model transformations | [mandatory] | R2,R3 |

EARS guards: R1 fixtures never use the primary Claude profile.
CLOSED: network, UI, tray, and process behavior are outside this test module.
Preconditions: pytest supplies writable temporary paths. Failure modes: assertions identify contract drift.
"""

import json
from pathlib import Path

import pytest

from hitch.config import ClaudePaths, GatewayProfile, apply_profile, new_profile_id, remove_profile, set_mode, snapshot
from hitch.models import Model, entries_for
from hitch.state import HitchState, StateStore


def test_sandbox_override_and_default_paths(tmp_path: Path) -> None:
    """Verify explicit sandbox paths override platform defaults.

    Require: writable temporary path.
    Guarantee: all sandbox-derived paths stay below override.
    Maintain: default derives from LOCALAPPDATA.
    Assert: no real profile path is touched.
    """
    sandbox = tmp_path / "Claude-3p"
    assert ClaudePaths.detect({"HITCH_CLAUDE_DIR": str(sandbox), "LOCALAPPDATA": "ignored"}).meta_file == sandbox / "configLibrary" / "_meta.json"
    assert ClaudePaths.detect({"LOCALAPPDATA": "C:/Local"}).data_dir == Path("C:/Local") / "Claude-3p"
    assert ClaudePaths.detect({"USERPROFILE": "C:/Users/fixture"}).data_dir == Path("C:/Users/fixture/AppData/Local/Claude-3p")
    assert ClaudePaths.at(tmp_path).desktop_config == tmp_path / "claude_desktop_config.json"


def test_apply_preserves_unknown_and_sealed_key_then_removes(tmp_path: Path) -> None:
    """Verify apply preserves Claude data and remove restores prior selection.

    Require: isolated data root.
    Guarantee: unknown profile fields and sealed key survive a keyless update.
    Maintain: prior entry can be restored.
    Assert: final removal preserves the other profile.
    """
    paths = ClaudePaths.at(tmp_path / "Claude-3p")
    other = "c2898676-28c3-4ef2-90bb-6a10501a33c1"
    paths.library_dir.mkdir(parents=True)
    paths.meta_file.write_text(json.dumps({"appliedId": other, "entries": [{"id": other, "name": "Default"}]}), encoding="utf-8")
    paths.desktop_config.write_text(json.dumps({"deploymentMode": "1p", "preferences": {"sidebarMode": "chat"}}), encoding="utf-8")
    profile_id = new_profile_id()
    apply_profile(paths, profile_id, GatewayProfile("https://hub.oxen.ai/api/ai", "secret", entries_for([], [])))
    body = json.loads(paths.profile_file(profile_id).read_text(encoding="utf-8"))
    body.update({"inferenceGatewayApiKey": "__sealed__:v1:abc", "somethingClaudeAdded": True})
    paths.profile_file(profile_id).write_text(json.dumps(body), encoding="utf-8")
    apply_profile(paths, profile_id, GatewayProfile("http://localhost:3001/api/ai", None))
    after = json.loads(paths.profile_file(profile_id).read_text(encoding="utf-8"))
    assert after["inferenceGatewayApiKey"] == "__sealed__:v1:abc"
    assert after["somethingClaudeAdded"] is True
    assert after["inferenceGatewayBaseUrl"] == "http://localhost:3001/api/ai"
    remove_profile(paths, profile_id, other, "1p")
    assert snapshot(paths).applied_id == other
    assert not paths.profile_file(profile_id).exists()


def test_malformed_desktop_blocks_mutation(tmp_path: Path) -> None:
    """Verify malformed strict configuration is never overwritten.

    Require: isolated path.
    Guarantee: mutation raises before changing malformed bytes.
    Maintain: no profile is created.
    Assert: source text remains exact.
    """
    paths = ClaudePaths.at(tmp_path)
    paths.data_dir.mkdir(exist_ok=True)
    paths.desktop_config.write_text("{ not json", encoding="utf-8")
    with pytest.raises(ValueError):
        apply_profile(paths, new_profile_id(), GatewayProfile("https://hub.oxen.ai/api/ai", "secret"))
    assert paths.desktop_config.read_text(encoding="utf-8") == "{ not json"
    assert not paths.library_dir.exists()


def test_empty_existing_key_does_not_authorize_apply(tmp_path: Path) -> None:
    """Verify an empty stored key is treated as absent.

    Require: isolated profile path.
    Guarantee: keyless apply raises for empty credentials.
    Maintain: existing profile bytes remain unchanged.
    Assert: no desktop config is created after refusal.
    """
    paths = ClaudePaths.at(tmp_path)
    profile_id = new_profile_id()
    paths.library_dir.mkdir(parents=True)
    paths.profile_file(profile_id).write_text(json.dumps({"inferenceGatewayApiKey": ""}), encoding="utf-8")
    with pytest.raises(ValueError):
        apply_profile(paths, profile_id, GatewayProfile("https://hub.oxen.ai/api/ai", None))
    assert json.loads(paths.profile_file(profile_id).read_text(encoding="utf-8"))["inferenceGatewayApiKey"] == ""
    assert not paths.desktop_config.exists()


def test_models_state_and_mode_contracts(tmp_path: Path) -> None:
    """Verify model ordering, state atomic round trip, and deployment mode clearing.

    Require: temporary writable root.
    Guarantee: Anthropic comes first and state excludes credentials.
    Maintain: unrelated desktop preferences survive mode changes.
    Assert: clear mode removes only deploymentMode.
    """
    models = [Model("gpt-5-mini", "GPT 5 Mini", "0b9f6c1e-7a52-4d3e-9f10-2c5a8e4b7d61", "openai"), Model("claude-opus-5-5", developer="anthropic"), Model("kimi-k3", developer="moonshot")]
    entries = entries_for([], models)
    assert [entry.name for entry in entries] == ["claude-opus-5-5", "claude-via-0b9f6c1e-7a52-4d3e-9f10-2c5a8e4b7d61", "claude-via-3k-imik"]
    store = StateStore(tmp_path / "state" / "hitch.json")
    state = HitchState(profile_id="abc", models=["kimi-k3"], catalog=models, previous_mode="1p")
    store.save(state)
    assert store.load() == state
    assert json.loads(store.path.read_text(encoding="utf-8"))["profile_id"] == "abc"
    assert "secret" not in store.path.read_text(encoding="utf-8")
    paths = ClaudePaths.at(tmp_path / "Claude-3p")
    paths.data_dir.mkdir(exist_ok=True)
    paths.desktop_config.write_text(json.dumps({"deploymentMode": "3p", "preferences": {"sidebarMode": "chat"}}), encoding="utf-8")
    set_mode(paths, None)
    assert json.loads(paths.desktop_config.read_text(encoding="utf-8")) == {"preferences": {"sidebarMode": "chat"}}

"""Convert Oxen catalog records into Claude Desktop model entries.

| Stage | Mechanism | Tag | Ref |
| --- | --- | --- | --- |
| INGEST | Accept catalog dictionaries | [contract] | R3 |
| SELECT | Prefer requested order or Anthropic-first catalog order | [mandatory] | R3 |
| RENDER | Produce Claude inference model JSON | [mandatory] | R3 |
| ALIAS | Derive stable wire IDs and retain LiteLLM model labels | [mandatory] | L2 |

EARS guards: R3 WHEN a model is not Anthropic, alias it with its UUID or reversed id.
CLOSED: no gateway requests occur here.
Preconditions: model ids are strings. Failure modes: invalid UUIDs are treated as absent.
"""

from __future__ import annotations

from dataclasses import dataclass
from hashlib import sha256
from typing import Any, Iterable, Optional

BEHAVES_AS = "claude-sonnet-5"


def litellm_alias(model_id: str) -> str:
    """Derive the same Claude alias for an identical upstream ID on every run.

    Args:
        model_id: Exact LiteLLM group name.
    Returns:
        Claude-compatible name with a lowercase hexadecimal suffix.
    """
    return "claude-model-" + sha256(model_id.encode("utf-8")).hexdigest()[:20]


@dataclass(frozen=True)
class Model:
    """Gateway model used by the profile writer.

    Require: ``id`` is the gateway-visible identifier.
    Guarantee: fields retain values needed to build a Claude entry.
    Maintain: absent metadata stays ``None``.
    Assert: no network or filesystem action occurs.
    """

    id: str
    name: Optional[str] = None
    uuid: Optional[str] = None
    developer: Optional[str] = None

    @property
    def display_name(self) -> str:
        """Return the provider's label, falling back to its original model ID."""
        return self.name or self.id


@dataclass(frozen=True)
class ModelEntry:
    """One Claude ``inferenceModels`` entry.

    Require: ``name`` is a gateway model name.
    Guarantee: ``to_config`` returns JSON-compatible data.
    Maintain: a plain entry has no label or behavior override.
    Assert: rendering does not mutate the entry.
    """

    name: str
    label: Optional[str] = None
    behaves_as: Optional[str] = None

    def to_config(self) -> Any:
        """Render the entry in Claude's accepted JSON shape.

        Require: fields satisfy this dataclass contract.
        Guarantee: returns a string or object suitable for JSON serialization.
        Maintain: plain entries remain bare strings.
        Assert: no input object is changed.
        """
        if self.label is None and self.behaves_as is None:
            return self.name
        item: dict[str, str] = {"name": self.name}
        if self.label is not None:
            item["labelOverride"] = self.label
        if self.behaves_as is not None:
            item["behavesAs"] = self.behaves_as
        return item


def entry_for(model: Model, alias_non_anthropic: bool = True) -> ModelEntry:
    """Create the Claude representation for one gateway model.

    Require: ``model.id`` is nonempty.
    Guarantee: Anthropic models retain their id; other models are aliased only for Oxen.
    Maintain: UUID aliases are lowercase and valid UUIDs only.
    Assert: no catalog ordering is changed.
    """
    if _is_anthropic(model):
        return ModelEntry(model.id)
    if not alias_non_anthropic:
        return ModelEntry(model.id, model.name or model.id, BEHAVES_AS)
    handle = model.uuid.lower() if model.uuid and _is_uuid(model.uuid) else model.id[::-1]
    return ModelEntry(f"claude-via-{handle}", model.name or model.id, BEHAVES_AS)


def entries_for(chosen: Iterable[str], catalog: Iterable[Model], alias_non_anthropic: bool = True) -> list[ModelEntry]:
    """Create the model list Claude should receive.

    Require: chosen ids and catalog ids are strings.
    Guarantee: explicit choices preserve caller order; empty choices prioritize Anthropic.
    Maintain: removed catalog models remain plain requested entries.
    Assert: catalog records are not mutated.
    """
    wanted = list(chosen)
    known = list(catalog)
    if not wanted:
        return [entry_for(model, alias_non_anthropic) for model in known if _is_anthropic(model)] + [entry_for(model, alias_non_anthropic) for model in known if not _is_anthropic(model)]
    by_id = {model.id: model for model in known}
    return [entry_for(by_id[item], alias_non_anthropic) if item in by_id else ModelEntry(item, behaves_as=BEHAVES_AS if not alias_non_anthropic else None) for item in wanted]


def _is_anthropic(model: Model) -> bool:
    """Determine whether Claude may use the gateway id unchanged.

    Require: model metadata follows ``Model``.
    Guarantee: explicit developer wins over the id prefix fallback.
    Maintain: matching is case-insensitive for developers.
    Assert: no state is modified.
    """
    return model.developer.lower() == "anthropic" if model.developer is not None else model.id.startswith("claude-")


def _is_uuid(value: str) -> bool:
    """Validate the UUID shape used by Oxen aliases.

    Require: value is text.
    Guarantee: only hexadecimal 8-4-4-4-12 values pass.
    Maintain: non-UUID text remains usable through id reversal.
    Assert: no normalization occurs here.
    """
    return len(value) == 36 and all(char == "-" if index in (8, 13, 18, 23) else char in "0123456789abcdefABCDEF" for index, char in enumerate(value))

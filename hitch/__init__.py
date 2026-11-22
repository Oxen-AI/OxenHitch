"""Hitch Python Windows backend package.

| Stage | Mechanism | Tag | Ref |
| --- | --- | --- | --- |
| CONFIG | Read and atomically mutate Claude profiles | [mandatory] | R1 |
| STATE | Persist non-secret Hitch state | [mandatory] | R2 |
| MODELS | Convert Oxen models to Claude entries | [mandatory] | R3 |

EARS guards: R1 sandbox paths remain below ``HITCH_CLAUDE_DIR``; R2 state excludes
credentials; R3 unknown model fields do not affect the conversion.

CLOSED: Rust is not imported or invoked by this package.

Preconditions: callers provide filesystem paths they are authorized to modify.
Failure modes: filesystem and malformed-JSON errors are raised at mutation boundaries.
"""

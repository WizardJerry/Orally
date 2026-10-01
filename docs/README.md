# Orally Documentation

[English](README.md) | [简体中文](README.zh-CN.md)

> Status: Reference

This directory separates current implementation facts, active refactoring work,
stable product language, and deferred design decisions. A document being
detailed does not mean its described feature is scheduled.

## Reading Order

1. Read the repository [README](../README.md) for commands and behavior that
   exist now.
2. Read the [Refactor Architecture Baseline](engineering/refactor-baseline.md)
   for the active refactoring scope, constraints, and sequence.
3. Read [CONTEXT](../CONTEXT.md) when naming product concepts.
4. Read the relevant [accepted ADR](adr/) before changing a decision it covers.
5. Read the [product documents](product/) for product intent, privacy direction,
   and deferred possibilities.

## Authority By Question

There is no single document that overrides every other document. Use the source
that owns the kind of question being asked:

| Question | Source |
| --- | --- |
| What does the repository do today? | Code, existing tests, and root [README](../README.md) |
| What does a product term mean? | [CONTEXT](../CONTEXT.md) |
| Why was a difficult architecture choice made? | The relevant accepted ADR |
| What is in scope for the current refactor? | The [Refactor Architecture Baseline](engineering/refactor-baseline.md) after approval |
| What user value and privacy direction are intended? | [Product documents](product/) |
| What work is authorized now? | The single active baseline; GitHub Issues are candidates, not authorization |

When sources conflict, do not silently merge their meanings. Record the
conflict in the active baseline and resolve it before changing behavior.

An accepted ADR constrains a feature when that feature enters implementation.
It does not automatically place the feature in the current milestone. A new ADR
must supersede an accepted ADR if the underlying decision changes.

## Document Status

- **Draft**: proposed content awaiting user review; does not authorize
  implementation.
- **Active**: directs current work.
- **Reference**: stable context that current work must respect.
- **Deferred**: deliberately outside the active scope.
- **Historical**: retained to explain how a decision evolved.

## Language Policy

- Existing English paths are canonical, including `README.md`, `privacy.md`,
  and ADR filenames.
- A Simplified Chinese pair uses the `.zh-CN.md` suffix, such as
  `README.zh-CN.md` or `privacy.zh-CN.md`.
- Repository-owned user, product, engineering, and decision documents are kept
  as English/Chinese pairs. Agent-only files under `.agents/` remain English so
  machine instructions do not drift.
- The `docs/product/grilling/README.md` directory guide is bilingual. Individual
  exploratory Grilling outputs may remain English until a conclusion is
  promoted into a product document, baseline, or ADR.
- Status values, commands, paths, configuration keys, environment variables,
  error text, and code identifiers remain unchanged across languages.
- A semantic change to a canonical document and its translation belongs in the
  same reviewable change.

Run the documentation contract check from the repository root:

```powershell
.\scripts\check-docs.ps1
```

## Change Discipline

Before implementing a new product capability:

1. Re-read the product definition and privacy document.
2. Check the relevant terms in [CONTEXT](../CONTEXT.md).
3. Check accepted and superseded ADRs for the area.
4. Update the active baseline with the problem, scope, and explicit non-goals.
5. Only then begin implementation.

The completed shared-speech milestone changed internal responsibility placement
without adding user-facing behavior. The bilingual-documentation milestone then
established paired review sources. Any following cleanup, contract-test stage,
or feature remains a separate reviewed step in the Active baseline.

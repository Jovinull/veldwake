# Definition of done

Status: **Required** for agents and human contributors.

A material change is done only when the implementation and the knowledge needed to maintain it are both durable.

## Checklist

- [ ] Scope and acceptance behavior are explicit.
- [ ] Existing implementation, docs, ADRs, TODOs, and dependencies were searched first.
- [ ] The smallest coherent implementation is complete; no hidden fallback masks failure.
- [ ] Code is formatted and applicable lints/build/tests were actually run.
- [ ] Tests cover behavior and regressions at the appropriate level.
- [ ] Performance-sensitive work has a defined fixture/budget and measured evidence; otherwise no performance claim is made.
- [ ] Security/untrusted-input implications and dependency provenance were reviewed.
- [ ] Current docs, examples, setup, commands, and API/format/protocol descriptions are accurate.
- [ ] Structural decisions have an ADR; reusable smaller discoveries are in `LEARNINGS.md`.
- [ ] New risks/issues are in the risk register or known issues, with owner/next action when known.
- [ ] Breaking changes, migrations, cache invalidation, and compatibility are explicit.
- [ ] `PROJECT_STATE.md` and `HANDOFF.md` reflect materially changed state.
- [ ] Generated artifacts were regenerated from their canonical source, not hand-edited.

## Gate reporting

Use only:

- **PASS:** executed and succeeded;
- **FAIL:** executed and failed, with relevant evidence;
- **BLOCKED:** required but unable to execute, with concrete blocker;
- **NOT YET APPLICABLE:** the system/capability does not exist or the gate has no meaningful subject.

Never translate “not run” into PASS. A platform-specific check can pass on one host while remaining unverified elsewhere.

## Review evidence

A handoff or PR should list commands, tool versions when relevant, result status, important output, changed contracts, and remaining risk. Visual/performance changes include comparable artifacts or metrics, not adjectives.

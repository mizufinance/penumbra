---
name: Point release
about: Checklist for preparing a point release for tooling
title: Release vX.Y.Z, via point-release
labels: ''
assignees: ''
---

# Tooling Release

<!--
Explain the rationale for this release: did a particular bugfix land that we want to ship quickly?
Is an external consumer blocked on a new RPC?
-->
In order to ship some minor improvements and bug fixes, let's prepare a `vX.Y.Z.` release, flushing out the current contents of the main branch.

## Changes to include
<!--
Explain the rationale for this release: did a particular bugfix land that we want to ship quickly?
Is an external consumer blocked on a new RPC?
-->

- [ ] Everything on current main
- [ ] Feature foo in PR: 
- [ ] Feature bar in PR: 

## Integration checks

- [ ] Identify affected host/protocol interfaces and required Bankd changes.
- [ ] Record the exact source revision and staged artifact verification.
- [ ] Record ordinary and release/prover checks actually run.

Internal prototype formats follow [the contract policy](../../AGENTS.md).

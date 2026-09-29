---
name: space-tree-optimize
description: Improve SpaceTree's scan, treemap, UI, reliability, privacy, security, or CI in a measured and reviewable change. Use for an explicit product optimization or quality sweep.
---

# Space-Tree Optimize

Improve the requested SpaceTree behavior with evidence from the current code and app. Keep independent concerns in separate changes or draft PRs. Continue through the requested scope; do not turn a broad request into one token edit.

1. Read `AGENTS.md`, the relevant source and tests, the README idea under consideration, and affected workflows. Check the behavior in the current app when it is user visible. A README item or old screenshot is a lead, not a reproducer.
2. Name the concrete defect, usability cost, security path, or measured expense before changing code. If the request is broad, rank the observed candidates by user impact and choose one coherent concern per change. An already satisfied item earns no code change.
3. Preserve allocated-byte accounting, APFS clone handling, symlink boundaries, selection and zoom behavior, and local scanning. For security work, show the actual unsafe path or advisory and test the correction. Use synthetic data in tests; do not put personal paths or scanned contents in logs or PR artifacts.
4. For speed work, compare the same fixture and output before and after. Include scanning, layout, or rendering costs as applicable. Keep correctness checks beside timing results. For interface work, use [SpaceTree design](../space-tree-design/SKILL.md).
5. Add a behavioral regression test for a bug. Update an existing GitHub Action only when the change needs a missing check or fixes an evidenced workflow fault. Use [SpaceTree verification](../space-tree-verify/SKILL.md) for the affected checks.
6. Report the observed before and after behavior, the exact checks run, and any boundary not exercised. If opening PRs was requested, make each draft narrow enough to review on its own.

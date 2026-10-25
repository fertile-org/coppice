---
name: github-ci
description: Check GitHub Actions workflow runs and their logs through the github__* tools. Load when a task asks why CI failed, whether a branch is green, or to read a job's logs.
---

# GitHub CI

Use the `github__*` tools exposed by the Coppice MCP gateway. Check `tools/list` for the exact names before calling.

## Diagnosing a failure

- Identify the failing workflow run and job first. Do not read every log.
- Quote the first real error line, not the cascade of later failures.
- Compare with the last green run on the same branch or on the default branch before blaming the change.

## Rerunning

- Rerun a job only when the task asks, or when the failure is plainly infrastructure (for example a runner timeout). Say which case it was.
- Do not edit workflow files to make checks pass.

## Report

State the run URL, the failing job, the first error line, and whether it looks caused by the branch or by the environment.

# Example Skeleton

A minimal one-input template using the built-in `rhei` machine and a single-file plan:

```yaml
# release-notes/template.yaml
name: release-notes
version: 1.0
description: Draft, review, and publish release notes for a version

inputs:
  - name: version
    description: Semantic version being released (e.g., 1.4.0)
    type: string
    validate: "^\\d+\\.\\d+\\.\\d+$"
  - name: channel
    description: Publication channel
    default: beta
```

```markdown
# release-notes/plan.rhei.md
# Rhei: Release {{version}} notes

## Tasks

### Task draft: Draft notes for {{version}}
**State:** pending

Draft the release notes for `{{version}}` targeting the `{{channel}}` channel.
Include highlights, breaking changes, and migration notes.

### Task review: Review draft
**State:** pending
**Prior:** Task draft

Review the draft for accuracy, tone, and completeness.

### Task publish: Publish to {{channel}}
**State:** pending
**Prior:** Task review

Publish the reviewed notes to the `{{channel}}` channel.
```

```bash
rhei instantiate release-notes --set version=1.4.0 --set channel=stable --output ./releases/1.4.0/
```

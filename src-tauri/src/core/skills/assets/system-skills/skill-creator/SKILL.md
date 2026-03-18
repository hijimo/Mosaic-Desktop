---
name: skill-creator
description: Create new skills with proper structure and metadata.
version: "1.0"
---

# Skill Creator

This system skill helps you create new skills with the correct directory structure and SKILL.md format.

## Usage

When the user asks to create a new skill, scaffold the following structure:

```
<skill-name>/
  SKILL.md        # Skill definition with YAML frontmatter
  scripts/        # Optional automation scripts
  assets/         # Optional icons and images
```

## SKILL.md Format

```yaml
---
name: <skill-name>
description: <one-line description>
version: "1.0"
triggers:
  - <trigger phrase>
---
```

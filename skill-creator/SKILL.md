---
name: skill-creator
description: Create, review, validate, and evolve Manus-compatible skills inside the file-seal repository. Use when adding a new skill directory, updating an existing SKILL.md, designing bundled references or scripts, or checking a skill before commit.
---

# Skill Creator for file-seal

## Purpose

Create small, self-contained Manus skills that provide reusable procedures for this repository. Keep each skill focused on one capability, avoid duplicating information already available from the model, and make safety boundaries explicit.

## Skill layout

Every skill must have this minimum structure:

```text
<skill-name>/
└── SKILL.md
```

Add bundled resources only when they are reused:

```text
<skill-name>/
├── SKILL.md
├── scripts/       # deterministic, reusable automation
├── references/    # details loaded only for a matching task
└── templates/     # reusable output or configuration templates
```

Do not add README, CHANGELOG, generated build output, credentials, or unrelated project files inside a skill.

## Creation workflow

1. Clarify the skill's trigger, user-facing outcome, and safety boundaries.
2. Inspect the repository to ensure the target skill directory does not already exist.
3. Initialize a new skill with the canonical initializer when creating from scratch:

   ```bash
   python3 /home/ubuntu/skills/skill-creator/scripts/init_skill.py <skill-name> --output-dir <repo-root>
   ```

4. Replace all initializer placeholders. Delete unused example scripts, references, and templates.
5. Write the skill with the following sections as applicable:
   - purpose and trigger conditions;
   - quick-start workflow;
   - decision points and failure handling;
   - repository-specific commands or API contracts;
   - references to bundled resources;
   - validation and delivery checklist.
6. Keep `SKILL.md` under 500 lines. Move long, variant-specific material into one-level-deep `references/` files and link to them directly from `SKILL.md`.
7. Use imperative or infinitive language. Prefer short procedures and concrete commands over broad background explanations.
8. Never place secrets, personal tokens, private URLs, or machine-specific absolute paths in a committed skill.

## file-seal-specific guidance

When a skill operates on the file-seal codebase:

- Treat the Rust ledger format and hash-chain protocol as compatibility-sensitive.
- Read `Cargo.toml`, the relevant `src/` module, and the applicable GitHub Actions workflow before proposing code changes.
- Preserve the distinction between file-content SHA-256 and ledger-chain hashes.
- Do not claim that a local hash chain provides trusted time, authorship, legal evidence, or protection against an attacker who can rewrite the entire ledger.
- For ledger changes, require tests for malformed JSON, sequence gaps, modified records, deleted records, inserted records, reordered records, and legacy v1 compatibility.
- Prefer read-only inspection first. Do not rewrite a user's ledger or repository history without explicit authorization.
- For large files or GUI operations, consider background work and progress reporting rather than blocking the UI thread.

## Resource design

Add a resource only if it removes repeated work:

- Put deterministic transformations in `scripts/` and test them with representative inputs.
- Put protocol schemas, compatibility tables, and long examples in `references/`.
- Put repeated output skeletons in `templates/`.
- Keep the main `SKILL.md` as the routing layer; explain when each resource should be read.

## Validation

Before committing a new or changed skill, run:

```bash
python3 /home/ubuntu/skills/skill-creator/scripts/quick_validate.py \
  --path <repo-root>/<skill-name>
```

Then inspect the result manually:

- frontmatter has exactly one `name` and one `description`;
- description says what the skill does and when to use it;
- no `[TODO: ...]` placeholders remain;
- all referenced files exist;
- scripts are executable or invoked through an explicit interpreter;
- no generated caches or secrets are present;
- instructions do not contradict the repository's current implementation.

If the skill contains scripts, run a representative test for each script. If it contains only Markdown, validate the directory and inspect links.

## Commit checklist

Before creating a commit:

1. Review `git diff --check`.
2. Review `git diff -- <skill-path>` for accidental content.
3. Run the validator again after the final edit.
4. Run relevant repository tests when the skill changes code-generation or protocol guidance.
5. Use a focused commit message such as `docs: add skill-creator skill`.
6. Do not push or open a pull request unless the user has authorized the repository action.

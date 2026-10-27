# Specs Directory

Specifications of the JustIngredients bot. Feature specs describe what the code does today;
task plans describe planned work and record its progress.

## Directory Structure

```
specs/
├── README.md                     # This index
├── telegram-bot.md               # Feature: bot interface, dialogue, recipes
├── database-design.md            # Feature: data persistence
├── deployment.md                 # Feature: Fly.io, CI/CD, health checks, metrics
├── llm-extraction.md             # Feature: ingredient extraction with Gemini
└── tasks/
    ├── llm-extraction-migration.md       # Tesseract → Gemini migration
    ├── ux-improvements.md                # UX plan (phases U0–U4)
    ├── measurement-standardization-tasks.md  # Canonical units
    └── improvement-backlog.md            # Verified code-quality items (IMP-xxx)
```

## Feature Specifications
- [Telegram Bot](telegram-bot.md): commands, photo flow, review and save, recipe list, callback
  data, dialogue states, errors, localization.
- [Database Design](database-design.md): schema, owner-scoped queries, migrations, cache.
- [Deployment](deployment.md): container, Fly configuration, health endpoints, secrets, CI/CD,
  monitoring.
- [Ingredient Extraction](llm-extraction.md): Gemini client, image preparation, guard, errors,
  metrics. It replaced the OCR specs (Tesseract removed in migration Task 5.1).

## Task Plans
- [LLM Extraction Migration](tasks/llm-extraction-migration.md): Gemini extraction (in
  production), Tesseract removal and pipeline (Phase 5 done; Tasks 5.2 and 5.4 wait for their
  review).
- [UX Improvements](tasks/ux-improvements.md): messages, navigation, recipe creation, recipe
  list, search.
- [Measurement Standardization](tasks/measurement-standardization-tasks.md): canonical units
  (tasks 1–2 done).

## Backlog
- [Improvement Backlog](tasks/improvement-backlog.md): prioritized, verified items to pull
  into 1-hour tasks.

## Maintenance Guidelines
- Feature specs describe the current code only. Planned changes go into a task plan, linked
  from the feature spec.
- Update the feature spec in the same commit as a behaviour change (commands, states, callback
  data, endpoints, secrets, workflows).
- Task plans are updated in place: tick tasks, add a short "Done:" note, record review gates.
- A spec that no longer matches the code is marked superseded at the top, then deleted.
- Every relative link must point to an existing file.

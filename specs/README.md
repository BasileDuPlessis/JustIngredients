# Specs Directory

Specifications of the JustIngredients bot. Feature specs describe what the code does today;
task plans describe planned work and record its progress.

## Directory Structure

```
specs/
├── README.md                     # This index
├── telegram-bot.md               # Feature: bot interface, dialogue, recipes
├── database-design.md            # Feature: data persistence
├── deployment.md                 # Feature: Fly.io, CI/CD, health checks, metrics, alerts
├── llm-extraction.md             # Feature: ingredient extraction with Gemini
└── tasks/
    ├── ux-improvements.md                # UX plan (phases U0–U9), its review log
    ├── ux-thread-wireframes.md           # Design of phase U6 (one active message)
    ├── measure-conversion.md             # Design of phase U8 (unit views, conversion table)
    ├── recipe-notes.md                   # Study and design of the recipe note (short free text per recipe)
    ├── linked-recipes.md                 # Design of phase U9 (linked recipes)
    ├── ingredient-sections.md            # Abandoned design, replaced by linked-recipes.md
    ├── observability.md                  # Plan O (Fly metrics, dashboards, alerts)
    ├── llm-extraction-migration.md       # Tesseract → Gemini migration
    ├── measurement-standardization-tasks.md  # Canonical units
    └── improvement-backlog.md            # Verified code-quality items (IMP-xxx)
```

## Feature Specifications
- [Telegram Bot](telegram-bot.md): commands, photo flow, review and save, recipe list, search,
  unit views, linked recipes, callback data, dialogue states, errors, localization.
- [Database Design](database-design.md): schema, owner-scoped queries, migrations, cache.
- [Deployment](deployment.md): container, Fly configuration, health endpoints, secrets, CI/CD,
  metrics, owner alerts and weekly report.
- [Ingredient Extraction](llm-extraction.md): Gemini client, image preparation, guard, errors,
  metrics. It replaced the OCR specs (Tesseract removed in migration Task 5.1).

## Task Plans
- [UX Improvements](tasks/ux-improvements.md): phases U0 to U9 done and in production (messages,
  navigation, recipe creation, recipe list, clean thread, search, OCR cleanup, unit views, linked
  recipes); later ideas at the end. Designs validated by the user:
  [thread wireframes](tasks/ux-thread-wireframes.md) (U6),
  [unit views](tasks/measure-conversion.md) (U8), [linked recipes](tasks/linked-recipes.md) (U9);
  abandoned: [ingredient sections](tasks/ingredient-sections.md).
- [Observability](tasks/observability.md): plan O, in production with v29; the user's last steps
  on Fly remain.
- [LLM Extraction Migration](tasks/llm-extraction-migration.md): Gemini extraction (in
  production), Tesseract removed; Tasks 5.2 and 5.4 not reviewed yet.
- [Measurement Standardization](tasks/measurement-standardization-tasks.md): canonical units
  (all tasks done).

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

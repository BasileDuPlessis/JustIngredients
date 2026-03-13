# Specs Directory

This directory contains project specifications and design documents. All specs are versioned and maintained as living documentation.

## Directory Structure

```
specs/
├── README.md                    # Management guidelines and spec types
├── ocr-processing.md           # Feature: OCR text extraction
├── telegram-bot.md             # Feature: Bot user interface  
├── database-design.md          # Feature: Data persistence
├── deployment.md               # Feature: Infrastructure
└── tasks/                      # Task specifications
    ├── ocr-refactor-strategy.md    # Completed OCR modularization
    └── ...                         # Future task specs
```

## Versioning Convention

- Use semantic versioning: `MAJOR.MINOR.PATCH`
- **Feature specs**: Version when architecture changes (v2.0 for breaking changes)
- **Task specs**: Usually keep only current version, update in-place for progress
- Keep version history **within** the spec document itself
- Archive old versions only for major milestones or significant changes

## Maintenance Guidelines

### When to Update Specs
- **Must Update**: Architecture changes, new requirements, breaking changes
- **Should Update**: Completed milestones, significant decisions, lessons learned
- **Don't Update**: Minor implementation details, routine bug fixes

### Versioning Strategy
- **Feature Specs**: Create new versions (v2.0.md) for major changes
- **Task Specs**: Update current version in-place for progress tracking
- **Archive**: Move old versions to `archive/` after 6-12 months

### How to Update
1. For **feature specs**: Create new version file when making breaking changes
2. For **task specs**: Update existing file with progress and completion markers
3. Add version history section within the spec document
4. Document implementation notes and decisions

## Current Specs

### Feature Specifications
- [OCR Processing](ocr-processing.md) - Text extraction and ingredient parsing from images
- [Telegram Bot](telegram-bot.md) - User interface and conversation management
- [Database Design](database-design.md) - Data persistence and full-text search
- [Deployment](deployment.md) - Infrastructure and production operations

### Task Specifications
- [OCR Refactor Strategy](tasks/ocr-refactor-strategy.md) - Completed modularization (all tasks done)

## Spec Types

### Feature Specs
High-level design documents for major application components. Include:
- Functional and non-functional requirements
- Architecture and data flow
- API design and interfaces
- Implementation constraints
- Testing and monitoring strategies

### Task Specs
Detailed implementation plans for specific work items. Include:
- Step-by-step execution plan
- Success criteria for each step
- Validation procedures
- Rollback plans

## Workflow

1. **Read feature specs** for architectural context and requirements
2. **Create task specs** in `/tasks/` for implementation work  
3. **Execute tasks** following success criteria
4. **Update feature specs** with lessons learned and architectural changes
5. **Version specs** when significant changes occur
6. **Archive completed task specs** after 3-6 months
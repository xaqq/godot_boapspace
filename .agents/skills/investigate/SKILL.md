---
name: investigate
description: Use ONLY when the user explicitly asks to investigate, research, or evaluate the feasibility of a feature, code change, or technical question. This skill is never auto-triggered; it requires a direct request like "investigate X", "is Y feasible", or "research Z".
---

# Investigate

Investigation-only mode focused on understanding, not implementing. Follow the
workflow below for any investigation task.

## Workflow

### 1. Clarify the question

Restate the user's goal in your own words to confirm alignment. Identify what
"feasible" means in this context: technical possibility, effort estimate, risk
level, or compatibility with existing architecture.

### 2. Explore the relevant code

Use glob, grep, and read tools to find:
- Related modules, classes, and functions
- Existing interfaces or APIs that would be affected
- Similar features or patterns already in the codebase
- Configuration, build system, or dependency constraints

Document file paths and line numbers for all relevant findings.

### 3. Identify constraints and risks

List everything that could block or complicate the change:
- Architectural limitations (e.g. Godot engine constraints, Rust GDExtension
  boundaries, `#[export]` vs dynamic node lookup)
- Dependency or version conflicts
- Performance implications
- Breaking changes to existing functionality
- Missing infrastructure (e.g. no test framework, no CI)

### 4. Estimate effort and scope

Provide a rough sizing:
- **Trivial** - single file, few lines
- **Small** - 1-2 files, under a day
- **Medium** - multiple files/modules, several days
- **Large** - significant refactor, weeks
- **Blocked** - not possible without upstream changes or major rearchitecture

### 5. Deliver the report

Conclude with a structured summary:

```markdown
## Feasibility Report: <topic>

**Verdict**: Feasible / Partially Feasible / Not Feasible

### Key Findings
- Finding 1 (reference file:line)
- Finding 2 (reference file:line)

### Constraints & Risks
- Risk 1
- Risk 2

### Effort Estimate
<Trivial | Small | Medium | Large | Blocked>

### Recommended Approach (if feasible)
Brief outline of the implementation path.

### Alternatives (if not feasible)
Other ways to achieve the same goal.
```

## Rules

- Do NOT write any implementation code. This is research only.
- Always cite file paths and line numbers for evidence.
- If the codebase lacks sufficient context to answer, say so rather than
  guessing.
- Prefer direct inspection of source files over assumptions.

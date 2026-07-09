# ADR-003: Template Extraction from Route Handlers

## Status
Accepted

## Context

All HTML templates are inline in route handlers using `maud::html!`. Route files are 100-300 lines with a 50/50 split between data fetching and template rendering. This couples rendering logic to HTTP handling and makes it impossible to test templates independently.

## Decision

Extract template rendering into dedicated template modules:

```
website/src/
  templates/
    layout.rs       — base_layout, reader_layout (already exists)
    series.rs       — series_detail template, series_list template
    reader.rs       — chapter_reader template
    home.rs         — home template
    source.rs       — source_list template
    mod.rs
  routes/
    home.rs         — thin handler: extract data → call template → return Html
    series.rs       — thin handler
    reader.rs       — thin handler
    source.rs       — thin handler
    check.rs        — JSON handlers (no templates)
    mod.rs
```

Each template function takes the data it needs (not `AppState` or `&Connection`) and returns `maud::Markup`.

```rust
// template function signature
pub fn series_detail(s: &Series, chapters: &[ChapterInfo]) -> Markup { ... }

// route handler becomes thin
pub async fn handler(State(state): State<Arc<AppState>>, Path(id): Path<i64>) -> Html<String> {
    let (series, chapters) = state.service.get_series_detail(id).await;
    Html(templates::series::series_detail(&series, &chapters).0)
}
```

## Consequences

**Easier**:
- Templates are testable — feed data, check HTML output
- Route handlers are readable — just orchestration
- A designer could edit templates without touching HTTP logic

**Harder**:
- More files, more boilerplate for each page
- Template functions need explicit parameter types (can't just use "whatever the route fetched")

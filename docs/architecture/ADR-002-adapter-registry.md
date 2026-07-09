# ADR-002: Centralized Adapter Registry

## Status
Accepted

## Context

Currently, adapters are registered in two places:
- `scraper/src/main.rs` (CLI binary)
- `website/src/scraper_helper.rs` (background sync)

Adding a new source site requires editing both files. The adapter list is duplicated.

## Decision

Create a centralized `AdapterRegistry` in `mangaread-core` that both CLI and website use.

```rust
// mangaread-core/src/registry.rs
pub struct AdapterRegistry {
    adapters: Vec<Box<dyn SiteAdapter>>,
}

impl AdapterRegistry {
    pub fn default() -> Self { ... }  // registers all known adapters
    pub fn all(&self) -> &[Box<dyn SiteAdapter>] { ... }
}
```

The `SiteAdapter` trait moves from `scraper` into `mangaread-core` so the registry doesn't depend on the scraper crate.

The `scraper` crate only provides adapter implementations, not the trait or registry.

## Consequences

**Easier**:
- Adding a new source = one file change (the adapter impl + registry registration)
- CLI and website always stay in sync
- Third parties could register custom adapters

**Harder**:
- `mangaread-core` gains a `thirtyfour` dependency (the trait references `WebDriver`)

**Mitigation**: The trait can use a generic `Browser` trait or the `thirtyfour` dependency stays light (just the type reference).

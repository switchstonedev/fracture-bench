---
title: Fracture Rebranding & SwitchStone Organization Alignment
summary: Rebranding from CruciVec to Fracture, package distribution as fracture-bench, and repository migration to switchstonedev/fracture-bench.
type: adr
status: active
tags: [adr, branding, distribution, organization]
aliases: [adr-004, fracture-rebrand]
---

# ADR 004: Fracture Rebranding & SwitchStone Organization Alignment

## Context
The project originated as CruciVec under personal stewardship. To transition the tool into a production-grade benchmarking suite backed by SwitchStone (`github.com/switchstonedev`), the project requires cohesive identity alignment across repository naming, package registries, and binary commands.

## Decision
1. **Binary & Package Asymmetry:** Publish the root binary crate as `fracture-bench` on crates.io while compiling the primary CLI executable as `fracture` via the Cargo `bin` table with name = "fracture". This ensures `cargo install fracture-bench` provisions the concise command `fracture run ...`.
2. **Subsystem Renaming:** Crate namespaces updated to `fracture-core`, `fracture-pacer`, `fracture-telemetry`, `fracture-adapters`, and `fracture-bench` (located in `crates/fracture-cli`).
3. **README Branding:** Top header established as `# Fracture` with subtitle `> High-pressure vector database benchmarking by SwitchStone`.
4. **Upstream Ownership:** Migrate primary repository origin to `https://github.com/switchstonedev/fracture-bench.git`.

## Consequences
- Single-entrypoint reproduction scripts and CLI invocations now reference `fracture` instead of `crucivec`.
- Package registries (crates.io, GHCR) reserve matching handle `fracture-bench`.

## References
- System Architecture: [[wiki/architecture]] (relates_to)
- Root Knowledge Graph: [[wiki/index]] (relates_to)

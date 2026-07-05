# AGENTS.MD - rust/godot_bridge/src

This directory contains the Godot-facing Rust GDExtension classes for the project. Keep this layer thin: it should own Godot nodes, scene wiring, input, drawing, and signal dispatch, while durable game rules stay in `rust/game_engine`.

## Scope

- `lib.rs` registers the extension modules.
- Menus should go into `menu` module
- Information / control panel should go to `panel` module.
- World view and simulation-facing scene nodes should go to `world` module.

## Godot Rust Conventions

- Use typed Godot APIs and signal builders. Prefer `bind()` / `bind_mut()`, direct Rust method calls, and `signals().name().connect(...)` over `Gd::call("method_name", &[])`.
- Use `#[export]` fields with `OnEditor<Gd<T>>` for required child node and cross-node references that are assigned in scenes.
- Treat invalid `OnEditor<Gd<T>>` exported node references as developer scene-wiring errors. Do not add defensive `is_instance_valid()` checks around required `OnEditor` references in normal lifecycle code.
- Use `GString` or resource paths only where Godot APIs require them. Keep ordinary Rust logic on Rust types.
- Keep `Base<T>` as the final stored field and use `base()` / `base_mut()` for node operations.

## Scene Wiring

- When adding or renaming exported fields, update the matching scene file under `godot/`.
- Panel classes should update existing labels or controls rather than finding nodes by string paths at runtime.
- Prefer static node configuration in Godot scenes over Rust setup code. Properties like sizing,
  wrapping, margins, selection, scroll behavior, and other fixed UI presentation should live in
  `.tscn` files; Rust should set dynamic state and content.

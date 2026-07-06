# AGENTS.md — godot_boapspace

Godot 4.7 game with a Rust GDExtension (`godot = "0.5"`) and a Bevy ECS game engine.

## Project vision

- 2D colony builder where each entity is simulated.
- The player gives high-level orders, and the simulation resolves the details.
- The game spans multiple surfaces / areas / planets that run in isolation.
- Tech stack: Godot 4.7 for UI and rendering, Rust for simulation.
- Simulation is written in Rust using Bevy ECS; Godot stays responsible for UI,
  rendering, input, and engine integration.
- The `game_engine` crate may depend on Godot only for trivial bridge /
  serialization metadata, such as exported enum conversion. Durable game logic
  must stay independent of Godot APIs.


## Key design

- **Game logic (`game_engine`)**: Rust implementation of the game logic. Contains the whole game logic as a library.
- **Godot bridge (`godot_bridge`)**: A relatively thin GDExtension that allows Godot to use our `game_engine`.
  Owns one `GameSimulation` is responsible for advancing it (via `tick()`). UI component query the ECS `World` to retrieve relevant data.


## Commands

```bash
cargo build --manifest-path rust/Cargo.toml           # Build workspace
cargo test --manifest-path rust/Cargo.toml            # Run all tests
~/.local/bin/godot4 godot/project.godot --editor      # Open editor
```

## Git push

This is a solo-developer repo. It is fine to publish local `master` directly to
`origin/master` when asked to push.

The `origin` remote may be configured as SSH and can resolve to a read-only
deploy key. First try the normal push:

```bash
git push -u origin master
```

If GitHub rejects that with `Permission ... denied to deploy key`, use the
authenticated GitHub CLI token as a one-off HTTPS credential helper without
persistently changing the remote:

```bash
gh auth status
git fetch origin master
git log --oneline origin/master..master
git -c credential.helper= \
  -c credential.helper='!gh auth git-credential' \
  -c remote.origin.pushurl=https://github.com/xaqq/godot_boapspace.git \
  push -u origin master
```


## Godot Key facts

- Prefer strong types over strings: use typed method calls (`bind()`/`bind_mut()`, direct
  calls on `#[func]` methods) instead of `Gd::call("method_name", &[])`. Use `#[export]`
  fields of typed `Gd<T>` / `OnEditor<Gd<T>>` for child node references instead of
  `get_node("path")`. Only use `GString`/`StringName` where Godot APIs genuinely require
  strings (e.g. `change_scene_to_file`, resource paths).
- For Godot project assets under `res://`, load imported assets through `ResourceLoader`
  as Godot resources (`Texture2D`, `PackedScene`, etc.). Do not use `Image::load_from_file`
  for imported PNGs at runtime; it bypasses Godot's import/export pipeline and triggers
  export warnings. If pixel data is genuinely needed, import/load the asset as an Image
  resource or derive pixels from a loaded texture intentionally.
- Godot scene directory hierarchy and corresponding rust implementation should match. 


## Working agreements

- This project, particularly the `game_engine` library, should be correctly tested. Unit and integrations tests
  where relevant.
- Do not use GDSCript, always write any code in Rust.
- Code quality is important. Most of the code is and will be AI generated, but must still be readable and maintainable.
- Avoid making assumptions or adding things without asking; You are welcome to ask instead.
- Do not fix unrelated behavior while working on a scoped task just to make tests pass. Report the unrelated issue and ask before changing behavior outside the requested scope.
- When asking questions with the questions/request_user_input tool, do not set `autoResolutionMs`.
  Questions should wait for an answer unless the user explicitly allows auto-resolve.
- Do not hesitate to perform exploration task by fanning out subagents.

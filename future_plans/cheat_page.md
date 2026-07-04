# Cheat Page — Implementation Plan

**Goal:** Add a "Cheat" page accessible from the in-game pause menu. It shows all 4 resource types (Wood, Stone, Food, Gold) with current amounts and +/-100 buttons for each, a "Back to game" button, and Escape-to-close behavior.

## Navigation Flow

```
Game → Escape → IngameMenu → Cheats button → CheatPage (menu hides)
CheatPage → Escape or "Back to game" → Game (cheat page hides, back to gameplay)
```

---

## Files to Create

### 1. `rust/src/cheat_page.rs` — New Rust module

New `CheatPage` struct extending `Control` (same pattern as `IngameMenu` and `ResourceHeader`):

- **Exported fields:**
  - `wood_label`, `stone_label`, `food_label`, `gold_label: OnEditor<Gd<Label>>`
  - `wood_add_button`, `wood_remove_button: OnEditor<Gd<Button>>` (and likewise for stone, food, gold — 8 buttons total)
  - `back_button: OnEditor<Gd<Button>>`
- **Internal state:**
  - `manager: Option<Gd<ResourceManager>>`
  - `cached: ResourcesState` (for poll-based label updates)
- **`ready()`:**
  - Find `/root/ResourceManager` autoload
  - Connect all 8 +/- buttons to `ResourceManager`'s `add_*` / `remove_*` methods via signal callbacks
  - Connect `back_button` signal → `self.base_mut().hide()`
  - Read initial resource values, start `set_process(true)`
- **`process()`:**
  - Poll ResourceManager values, update labels only when changed (same polling pattern as ResourceHeader)
- **`unhandled_input()`:**
  - On Escape (pressed, non-echo): if visible, `hide()` and `accept_event()` to prevent GameWorld from also processing the Escape

### 2. `godot/cheat_page.tscn` — New scene

```
CheatPage (type="CheatPage")
├── VBoxContainer (centered, same dimensions as IngameMenu's VBoxContainer)
│   ├── TitleLabel (Label, font_size=36, text="Cheats")
│   ├── HSeparator
│   ├── HBoxContainer
│   │   ├── WoodLabel (Label, text="Wood: 0")
│   │   ├── WoodAddButton (Button, text="+100")
│   │   └── WoodRemoveButton (Button, text="-100")
│   ├── HBoxContainer
│   │   ├── StoneLabel (Label, text="Stone: 0")
│   │   ├── StoneAddButton (Button, text="+100")
│   │   └── StoneRemoveButton (Button, text="-100")
│   ├── HBoxContainer
│   │   ├── FoodLabel (Label, text="Food: 0")
│   │   ├── FoodAddButton (Button, text="+100")
│   │   └── FoodRemoveButton (Button, text="-100")
│   ├── HBoxContainer
│   │   ├── GoldLabel (Label, text="Gold: 0")
│   │   ├── GoldAddButton (Button, text="+100")
│   │   └── GoldRemoveButton (Button, text="-100")
│   ├── HSeparator
│   └── BackButton (Button, text="Back to game")
```

`process_mode = 3` (WhenPaused) on the root node. `node_paths` metadata listing all exported fields.

---

## Files to Modify

### 3. `rust/src/lib.rs` — Register module

```rust
mod cheat_page;   // <-- add this line
```

### 4. `rust/src/resources.rs` — Add remove methods

Add 4 `#[func]` methods to `ResourceManager`:

```rust
#[func]
fn remove_wood(&mut self, amount: u32) {
    self.state.wood = self.state.wood.saturating_sub(amount);
    self.base_mut().emit_signal("resources_changed", &[]);
}
// ... same pattern for stone, food, gold
```

### 5. `rust/src/ingame_menu.rs` — Add Cheats button and CheatPage reference

- Add two new exported fields:
  - `cheats_button: OnEditor<Gd<Button>>`
  - `cheat_page: OnEditor<Gd<CheatPage>>`
- Import `crate::cheat_page::CheatPage`
- In `ready()`: connect `cheats_button` signal → hide self, show `cheat_page`

### 6. `godot/ingame_menu.tscn` — Add Cheats button node

- Add `cheats_button` to `node_paths` metadata (append to PackedStringArray)
- Add `CheatsButton` child to VBoxContainer (before or after ContinueButton):
  ```
  [node name="CheatsButton" type="Button" parent="VBoxContainer"]
  layout_mode = 2
  text = "Cheats"
  ```

### 7. `rust/src/game_world.rs` — Add cheat_page reference

- Add exported field: `cheat_page: OnEditor<Gd<CheatPage>>`
- Import `crate::cheat_page::CheatPage`
- In `ready()`: add `self.cheat_page.clone().set_visible(false);` (alongside existing ingame_menu hide)

### 8. `godot/game_world.tscn` — Add CheatPage instance

- Add an `[ext_resource]` for `cheat_page.tscn`
- Add `CheatPage` node as child of `GameWorld` (sibling of `IngameMenu`):
  ```
  [node name="CheatPage" parent="." instance=ExtResource("3_cheat_page")]
  visible = false
  ...
  ```
- On the `IngameMenu` instance node, extend `node_paths` to include `cheats_button` and `cheat_page`, and add corresponding `NodePath` entries:
  - `cheats_button = NodePath("VBoxContainer/CheatsButton")`
  - `cheat_page = NodePath("../CheatPage")`

---

## Build & Verify

```bash
cargo build --manifest-path rust/Cargo.toml
```

Then open in Godot editor to verify the node paths resolve correctly and the scene loads without errors.

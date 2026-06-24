# Minecraft / Azalea 26.2 Port Notes

This package patches the main 26.2 compile blockers from the uploaded source:

- `open_inventory()` now handled as `Result<Option<ContainerHandle>, MissingComponentError>`.
- `open_container_at()` now handled as `Result<Option<ContainerHandle>, MissingComponentError>`.
- `client.world()` now unwrapped/handled before `.read()`.
- `selected_hotbar_slot()` now handled as a `Result<u8, MissingComponentError>`.
- `get_inventory()` / `get_held_item()` now handled as `Result` values.
- position/hunger component reads now call `.ok()` before converting to optional values.
- biome registry display was made conservative so it does not block compilation on changed registry-key APIs.

Build with:

```powershell
cargo build --release 2>&1 | Tee-Object build_error.txt
```

If new errors appear, send `build_error.txt`; they will be later-stage 26.2 API changes beyond the first uploaded error log.

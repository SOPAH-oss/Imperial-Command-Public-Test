# Minecraft 26.2 Build Notes

This package targets Minecraft's newer Minecraft protocol line by switching Azalea dependencies from crates.io `0.16.0+mc26.1` to the current Azalea GitHub source, whose upstream README lists Minecraft `26.2` as the current supported version.

## Important

- First build requires internet access so Cargo can fetch `https://github.com/azalea-rs/azalea`.
- Use Rust nightly, because Azalea recommends nightly for current builds.
- Do **not** build with `--locked` on the first build, because `Cargo.lock` was intentionally removed so Cargo can resolve the 26.2 git dependencies.

## Commands

```powershell
rustup install nightly
rustup default nightly
cargo clean
cargo build --release
```

After a successful build, the executable will be here:

```text
target\release\rust_pearl_stasis_bot.exe
```

Then you can run the included build script if desired:

```powershell
powershell -ExecutionPolicy Bypass -File .\build_version1.ps1
```

## Notes

This preserves the latest FAQ numbered lookup and join-greeter fixes from the previous branch.

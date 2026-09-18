# egui-noodle

Developed as a git submodule of Coollab (`src/packages/egui-noodle`) and a member of its Cargo workspace, but an independent, publishable crate: it must build, test and run its example when cloned alone.

## Rules

- **All of Coollab's coding guidelines apply in full** - `guidelines/GENERAL.md` in the parent repo (`../../../guidelines/GENERAL.md` from here). Same quality bar: never panic, `Result`-based errors, TDD for the model and the geometry, no abbreviations, the comment rules. Being a separate repo changes where commits land, nothing else.
- **One deviation: dependency versions are pinned here, never `{workspace = true}`.** A crate that must build when cloned alone cannot read a workspace it is not cloned with.
- **Nothing in here may know about Coollab** - no `NodeDefinitionId`, no ISF, no `compiler` crate. The check is the standalone example, run from a clone outside the Coollab tree: `cargo run --example demo`.
- `docs/shortcuts.md` is user documentation. Update it in the same commit as any gesture that adds or changes a binding.
- `post-mortems/` follows Coollab's convention, so the `/post-mortem` command's rules apply unchanged.
- Commits: gitmoji prefix, short subject, a body only when the subject cannot carry it, no `Co-Authored-By` line.

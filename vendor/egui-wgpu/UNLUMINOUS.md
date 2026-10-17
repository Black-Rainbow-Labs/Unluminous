# egui-wgpu 0.36.1, changed for Unluminous

This is egui-wgpu 0.36.1 from crates.io (egui commit `4c1f2fae95475a40e524884ebb298bcb1714b08e`), MIT OR
Apache-2.0, with one change. The workspace takes it through `[patch.crates-io]`, so eframe's own dependency gets
this copy. `unluminous.patch` is the whole difference from the published crate.

## What changed (`task-2288`)

1. **`WgpuConfiguration::scene_texture`**, off by default. With it on, `Painter::paint_and_update_textures`
   draws every frame into the capture texture and then copies that texture to the surface, which is what it
   already did on a frame being captured for a screenshot.
2. **`SceneTexture`** in the renderer's callback resources. Inserted before `update_buffers`, so a paint
   callback's `prepare` reads the **previous** frame. Unluminous blurs it for a modal's backdrop
   (`crates/unluminous-app/src/services/scene_blur.rs`).
3. **`CaptureState::copy_textures` is split** into `copy_to_buffer` and `copy_to_surface`, so a frame can be
   copied to the surface without a buffer being made for it. `copy_textures` still does both.
4. **The screenshot's pixel loop runs on a thread of its own** on native. It turned a 4K frame into `Color32`s
   inside the `map_async` callback, on whichever wgpu call on the window's thread serviced it, which measured about
   a tenth of a second of that thread.

Nothing else is different, and with `scene_texture` off the only change in behaviour is item 4.

## Carrying it to the next egui

Take the new egui-wgpu from crates.io into `vendor/egui-wgpu`, apply `unluminous.patch` with
`git apply --3way` or by hand, and run `cargo test -p unluminous-app --test screenshots` and the smoothness bench
(`tools/perf-smooth/`). When egui-wgpu gives paint callbacks the previous frame itself, delete this folder and the
`[patch.crates-io]` entry.

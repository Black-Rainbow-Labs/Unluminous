# wgpu-hal 30.0.1, changed for Unluminous

This is wgpu-hal 30.0.1 from crates.io, MIT OR Apache-2.0, with the one change below. The workspace takes it
through `[patch.crates-io]`, so wgpu's own dependency gets this copy. `unluminous.patch` is the whole difference
from the published crate.

## What changed (`task-2369`)

**A shader FXC compiled is cached as bytes, not as the `ID3DBlob` FXC returned.** `compile_fxc` in
`src/dx12/shader_compilation.rs` copies the blob's bytes into `CompiledShader::Precompiled` and the blob is released
on the thread that made it. `CompiledShader::Fxc` is no longer made.

The reason is that **d3dcompiler_47.dll's `ID3DBlob` does not count its references atomically.**
`Device::load_shader` keeps every compiled shader in a per device cache and returns `value.shader.clone()` on a hit.
The clone is an `AddRef` taken under the cache's lock, but every thread drops its copy (`Release`) outside the lock
once its pipeline is made. Two threads making the same pipeline on one device lose updates to the count, the blob is
freed while the cache still holds it, and the next clone reads freed memory.

Measured on this machine with `blob-refcount.ps1` in this folder (`pwsh -File vendor/wgpu-hal/blob-refcount.ps1`): one blob, 1000 extra references held, then 16
threads each doing 200,000 balanced `AddRef`/`Release` pairs. An atomic count ends at 1001. It ended at 1083 to 1624
in every run, for a blob from `D3DCreateBlob` and for one from `D3DCompile`.

What it cost: the window test binaries share eight devices between every test thread (`tests/common/mod.rs`,
`shared_render_state`), and every test makes egui's pipeline in `egui_wgpu::Renderer::new`. During `task-2364`, one
run of `notebooks` aborted with `misaligned pointer dereference` in `windows_core::Interface::assume_vtable`, and one
run of `canvas_realm` ended with no message at all. A scratch test with 32 threads making a renderer on the shared
devices at once reproduced the abort on its 61st run, with the backtrace `assume_vtable` ← `IUnknown::clone` ←
`wgpu_hal::dx12::Device::load_shader` (device.rs:415) ← `create_render_pipeline` ← `egui_wgpu::Renderer::new`. The shipped window
makes its pipelines on one thread, so it was not seen there, but a second thread making a pipeline would have
crashed it the same way.

DXC's blobs count atomically and are left alone. This machine has no `dxcompiler.dll`, so wgpu uses FXC here and
in the shipped window.

## Carrying it to the next wgpu

Take the new wgpu-hal from crates.io into `vendor/wgpu-hal`, apply `unluminous.patch`, and check whether
`compile_fxc` still returns the blob and `load_shader` still clones a cached shader. If upstream has stopped
sharing FXC blobs between threads, delete this folder and the `[patch.crates-io]` line instead.

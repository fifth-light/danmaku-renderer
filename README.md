# danmaku-renderer

My simple danmaku renderer. Can render many danmaku efficiently.

## Try it out

Just run `cargo run --example=wgpu_renderer --features=renderer-wgpu`.

## Backend

This renderer has two backends:

- Cairo (and GTK)
- wgpu

You should use wgpu backend, as it is hardware accelerated.


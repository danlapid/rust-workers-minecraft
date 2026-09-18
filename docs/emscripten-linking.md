# Emscripten linking investigation

The Rust Emscripten target supports `cdylib` output with its default
position-independent code (PIC) relocation model. This application's global
`-Crelocation-model=static` override conflicts with the side-module link that
Rust requests for a `cdylib`.

Static linking and static relocation are different choices. PIC object files can
still be combined into one self-contained executable. Supporting a dependency's
shared-library output does not require the Worker to load that library at runtime;
the executable consumes its `rlib`.

## Reproduced behavior

Tests on September 6, 2026 used the pinned Rust nightly and Emscripten frontend.

| Case | Target defaults | Forced static relocation |
| --- | --- | --- |
| `no_std` library exporting a static address | Builds | Relocation error |
| `std` library using a `Vec` | Builds | Relocation error |
| Executable using a dependency with `cdylib` and `rlib` outputs | Builds and runs | Dependency link fails |

The executable printed `DUAL-OUTPUT-CONSUMER-OK`. These cases do not use
wasm-bindgen. They distinguish the application's override from the target's
ordinary behavior.

## Binding generation for side modules

Removing the override exposes two additional failures in wasm-bindgen's
Emscripten path:

1. **Function identity during descriptor interpretation.** Side modules obtain
   function pointers through `GOT.func` imports. The interpreter initializes
   those globals to zero, which can make a closure descriptor select an unrelated
   function. Some exported functions have no initial table slot: the dynamic
   loader supplies one. Binding generation needs the function's identity, rather
   than an assumed runtime table index.
2. **Dynamic-link metadata placement.** Walrus emits the `dylink.0` custom section
   after the standard sections. The dynamic-linking ABI requires it first, and
   LLVM's object reader rejects the reordered result. Moving the section back
   makes the same object pass LLVM object parsing.

A small exported closure reproduces the first failure. An exported addition
function reaches the second. They reproduce independently of Pumpkin and Workers.

A local prototype preserves descriptor function identities and restores the
metadata's required position. It builds the closure example and unmodified
wasm-streams with both `cdylib` and `rlib` outputs. The CLI-support tests pass:
91 against current upstream wasm-bindgen and 83 against the pinned version.

An isolated copy of the complete Minecraft application also passed `npm test`
with the prototype CLI, default PIC relocation, and the unmodified pinned
wasm-streams source. It printed `PUMPKIN-DO-SQLITE-RESTART-OK` after checking
filesystem isolation, two-player gameplay, a shared block edit, and player/block
restoration. This validates the proposed replacement in the application's static
linking scenario.

A subsequent runtime regression loads a Rust side module and calls its generated
JavaScript bindings. This exposed missing runtime handling beyond the original
build fix: exported aliases still reached descriptor stubs, the side module had
its own exception tag, and generated calls used main-module-only variables.
The wasm-bindgen branch now redirects those aliases, preserves the shared tag,
and resolves calls through Emscripten's dynamic symbol map.

The test passes in debug and optimized release builds, with and without another
library loaded first. It verifies
stateful callbacks, JavaScript-to-Rust and Rust-to-JavaScript calls, exceptions,
explicit destruction, and rejection after destruction. The generated JavaScript
is used unchanged. These changes remain isolated from the active dependency
checkouts.

The Walrus metadata-order fix has merged as
[walrus#320](https://github.com/wasm-bindgen/walrus/pull/320) and shipped in 0.27.0.
The wasm-bindgen review branch now requires that published release; its CLI and
side-module runtime tests run without a local Walrus override.

## Downstream patch

The active application still uses its existing pins and workaround. Replacing
that workaround requires integrating the toolchain fixes and removing the static
relocation override together. The isolated test establishes that this path works
for Minecraft; it does not replace review and dynamic-loading coverage for the
toolchain changes. Removing `cdylib` from wasm-streams upstream would change its
outputs for other consumers without addressing these failures.

The implementation points are Rust's `EmLinker::set_output_kind`, wasm-bindgen's
descriptor interpreter and binding generator, and the emitted Wasm section order.
Cargo artifact filtering would avoid the failing path; it would not fix support
for shared modules.

[![License: AGPL v3](https://img.shields.io/badge/License-AGPL_v3-blue.svg)](LICENSE)
[![Status: pre-alpha](https://img.shields.io/badge/status-pre--alpha-orange.svg)]()
[![Platform: aarch64](https://img.shields.io/badge/platform-aarch64-lightgrey.svg)]()

# Kuberve Kernel

The kernel of **Kuberve** — an independent operating system for aarch64
(not Linux, not POSIX). Written in Rust (`no_std`), runs on QEMU `virt`
with `cortex-a72` during development.

> **Pre-alpha.** ABI is unstable, breaking changes happen between any two
> commits. No warranty. Do not use in production.

## What works

| Milestone | Feature | Status |
|---|---|---|
| v0.1 | Boot, UART, GICv3, timer | ✅ |
| v0.2 | Scheduler (states, priority, aging, sleep) | ✅ |
| v0.3 | MMU (4-level, ASID), phys alloc, kernel heap | ✅ |
| v0.4 | EL0, ELF loader, syscalls | ✅ |
| v0.5 | IPC (`Message`, queues, blocking send/recv) | ✅ |
| v0.6 | Userspace drivers (UART server, IRQ forwarding) | ✅ |
| v0.7 | Per-thread address spaces (code/stack/heap) | ✅ |
| v0.8 | NVMe + FAT32 + FD table | ✅ |
| v0.9 | Network: virtio-net, ARP, IPv4, ICMP, UDP, TCP, DHCP | ✅ |
| v0.10 | Graphics: virtio-gpu, framebuffer, console | ✅ |
| v0.11 | Input: virtio-keyboard, virtio-tablet | ✅ |
| v0.12 | Shell | ✅ |
| v0.13 | Package manager (`kv`) | ✅ |
| v0.14.4 | GUI rewrite (W.1–W.7: compositor, windows, drag, userspace API) | ✅ |

Full roadmap: [Kuberve/docs → ROADMAP.md](https://github.com/Kuberve/docs/blob/main/ROADMAP.md)

## Design highlights

- **Microkernel-inspired**: kernel holds only mechanisms (scheduler, MMU,
  IPC routing, IRQ dispatch, syscall entry). Drivers live in userspace
  (see `uart_server` in the `user` repo).
- **Per-thread address spaces** with ASID-tagged TLB. No `tlbi` needed
  (QEMU `virt` hangs on `tlbi vmalle1`).
- **Own ABI**: `x8` = syscall number, `x0..x5` = args, `x0` = return.
  `svc #0` to enter. Not POSIX.
- **Double-buffer compositor**: `FB_BACK` at `0x48200000`, `present_damage()`
  flushes only the dirty rectangle to `FB` and virtio-gpu.
- **CPU**: single-core for now, `cortex-a72`, QEMU `virt,gic-version=3`.

Architecture deep-dive: [Kuberve/docs → architecture/overview.md](https://github.com/Kuberve/docs/blob/main/architecture/overview.md)

## Prerequisites

- Rust nightly (for `#![feature(alloc_error_handler)]`)
- `aarch64-unknown-none` target: `rustup target add aarch64-unknown-none`
- QEMU ≥ 7.0 (`qemu-system-aarch64`)
- `mtools` (for `mcopy`/`mdir`, FAT32 image manipulation)

## Build

The kernel depends on `kuberve-abi` via a `git` dependency (pulled automatically by Cargo).

```bash
git clone https://github.com/Kuberve/kernel.git
cd kernel

# 1. Build userspace programs first (kernel loads them from FAT32)
cd ../user && cargo build --release
cp target/aarch64-unknown-none/release/init /tmp/init.elf
cp target/aarch64-unknown-none/release/uart /tmp/uart.elf
# ... (see user/README.md for the full list)

# 2. Copy binaries to the FAT32 image
cd ../kernel
mcopy -o -i nvme.img /tmp/init.elf ::/system/bin/init
mcopy -o -i nvme.img /tmp/uart.elf ::/system/bin/uart

# 3. Build the kernel
cargo build

# 4. Run
qemu-system-aarch64 -machine virt,gic-version=3 -cpu cortex-a72 -m 256 \
  -display gtk -serial mon:stdio \
  -drive file=nvme.img,if=none,id=nvme0,format=raw \
  -device nvme,serial=deadbeef,drive=nvme0 \
  -netdev user,id=net0,hostfwd=tcp::8081-:8080 \
  -device virtio-net-pci,netdev=net0,disable-legacy=on \
  -device virtio-gpu-pci,disable-legacy=on \
  -device virtio-keyboard-pci,disable-legacy=on \
  -device virtio-tablet-pci,disable-legacy=on \
  -kernel target/aarch64-unknown-none/debug/kuberve
```

Exit QEMU: `Ctrl+A` then `X`.

## Directory structure

```
src/
├── main.rs                # kmain: init order, idle loop
├── version.rs             # single source of truth for version strings
├── arch/aarch64/          # CPU-specific: vectors, GIC, MMU, timer, scheduler, syscall
├── drivers/               # kernel-side drivers: PCI, NVMe, UART, virtio (net/gpu/input/tablet)
├── fs/                    # FAT32, FD table, VFS
├── gfx/                   # compositor, cursor, console, shell, windows, text, font
├── ipc/                   # Message, Queue
├── lib/                   # kernel heap allocator, ELF loader, panic handler
└── network/               # ARP, IPv4, ICMP, UDP, TCP, DHCP, sockets
```

## Related repos (Kuberve Org)

- [kuberve-abi](https://github.com/Kuberve/abi) — syscall numbers, `Message`, shared types (MIT)
- [kuberve-libc](https://github.com/Kuberve/libc) — userspace library: syscall wrappers, heap, UART (LGPLv3)
- [kuberve-user](https://github.com/Kuberve/user) — userspace programs: `init`, `uart`, `tcp`, `kv`, `hw` (AGPLv3)
- [kuberve-docs](https://github.com/Kuberve/docs) — architecture, roadmap, devlog
- [kuberve-spec](https://github.com/Kuberve/spec) — formal ABI specs

## Contributing

Pull requests are accepted, but may take time to review. Please read
[CONTRIBUTING.md](CONTRIBUTING.md) and sign the [CLA](CLA.md) before opening
a PR.

Code comments in the existing codebase are mostly in Russian. New comments
and doc-comments (`///`) should be in English.

## License

- **Kernel code:** AGPL-3.0-or-later. See [LICENSE](LICENSE).
- **Syscall exception:** programs that call Kuberve syscalls are **not**
  derivative works of the kernel. See [LICENSE-EXCEPTION.md](LICENSE-EXCEPTION.md).
- **Commercial licensing:** available for closed-source use.
  See [COMMERCIAL_LICENSE.md](COMMERCIAL_LICENSE.md).
- **Trademark:** "Kuberve" and the logo are trademarks.
  See [TRADEMARK.md](TRADEMARK.md).
- **FAQ:** [LICENSE-FAQ.md](LICENSE-FAQ.md).

---

**Kuberve** is a trademark of the project author. The source code is licensed
under AGPLv3, but the name and logo may not be used to endorse derivative
products without written permission.

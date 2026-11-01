# Contributing to Kuberve

First of all, thank you for considering contributing to Kuberve! 

Kuberve is an independent, open-source operating system built on the idea that
users should truly own their hardware and software. We believe in building a
unified ecosystem where improvements are shared back with the community, rather
than locked away in proprietary jailbreaks or closed-source forks.

This project is currently maintained by a solo developer. I try to review PRs
and issues as quickly as possible, but please be patient. 

> **Pre-alpha warning:** The ABI is highly unstable. Breaking changes happen
> between any two commits. If you are writing a large feature, please open an
> issue to discuss it first to avoid wasted effort.

## How can I contribute?

### 1. Reporting Bugs
- **Search first:** Check if the issue has already been reported.
- **Be specific:** Include the OS version (`Kuberve v0.14.4`), the platform
  (QEMU `virt,gic-version=3`, cortex-a72, etc.), and the exact command that
  triggered the bug.
- **Logs & Screenshots:** For kernel panics, copy the UART log (especially the
  `!! sync exception !!` dump). For GUI issues, a screenshot is worth a
  thousand words.

### 2. Suggesting Features
Open an issue with the `[Feature]` prefix. Describe *why* this feature is
needed and *how* it aligns with the Kuberve philosophy (e.g., "User owns the
hardware", "No POSIX bloat"). If you plan to implement it yourself, mention
that so we can discuss the architectural approach before you start coding.

### 3. Submitting Code (Pull Requests)
- **One PR = One logical change.** Don't mix a bug fix with a new feature.
  If you touch more than 3 unrelated files, consider splitting the PR.
- **Branch naming:** `feat/gui-windows`, `fix/nvme-timeout`, `docs/readme-typos`.
- **Test your code:** Ensure `cargo build` and `cargo build --release` pass.
  If you are changing the kernel, boot it in QEMU and verify that `init`,
  networking, and GUI still work.
- **Formatting:** Run `cargo fmt` and `cargo clippy` before committing.

## Code Guidelines

### Language Policy
- **Existing code:** Most inline comments (`//`) in the current codebase are
  written in **Russian**. You do not need to translate them.
- **New code:** All new comments and doc-comments (`///`) **must be written in
  English**. 
- **Documentation:** README, specs, and docs are strictly in English.
- **Commit messages:** Must be in English, using the imperative mood
  (e.g., `add NVMe timeout handling`, not `added` or `adds`).

### Architecture & Quirks
- **No POSIX:** Do not add `fork()`, `read()`, `write()` just to match Unix.
  We have our own ABI. If you need a new syscall, add it to the `kuberve-abi`
  repo first, then implement it in the kernel.
- **QEMU Quirks:** 
  - Never use `tlbi vmalle1` (it hangs QEMU `virt`). Use ASID-tagged TLB.
  - `dc cvau` / `ic ivau` in `load_elf` are disabled for the same reason.
- **No dynamic allocations in syscalls:** Do not call `phys::alloc_page()` inside
  a syscall handler running in User AS. Use static `.bss` buffers
  (like `NvmeBuf`).

## Licensing and CLA (Important!)

Kuberve is dual-licensed:
- **Open Source:** AGPLv3 (kernel, user), LGPLv3 (libc), MIT (abi).
- **Commercial:** Available for companies that want to build closed-source
  products on top of Kuberve.

To make this legally possible, **we require all contributors to sign a
Contributor License Agreement (CLA)**. 

By signing the CLA, you grant the project author the right to license your
contribution under both the AGPLv3 and commercial license terms. **You still
retain the copyright to your contribution**, but this allows the project to
remain sustainable and legally protected.

**How to sign:** When you open a Pull Request, a GitHub bot will automatically
ask you to comment:
> `I have read the CLA and agree`
Once you do, your PR will be cleared for review. You can read the full
[CLA here](CLA.md).

## License

By contributing, you agree that your contributions will be licensed under the
same terms as the repository you are contributing to (AGPLv3, LGPLv3, or MIT),
and you grant the project a non-exclusive, perpetual license to relicense your
work under a commercial license.

---

**Kuberve is a trademark of the project author.**
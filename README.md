# cftokens

This repository hosts the code for building the cftokens binary that underlies the cfformat CommandBox module.

It uses a pinned [syntect](https://github.com/trishume/syntect) revision with `branch_point` support, plus the CFML and selected Sublime Text syntaxes in this repo. `build.rs` compiles those into an embedded newline-aware syntax pack.

Build the executable with Cargo:

```bash
cargo build --locked --release
```

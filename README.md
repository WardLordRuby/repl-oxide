[match_wire]: https://github.com/WardLordRuby/match_wire
[crossterm]: https://crates.io/crates/crossterm
[clap]: https://crates.io/crates/clap

# repl-oxide
repl-oxide is a work-in-progress, async-first REPL (Read-Eval-Print Loop) library for Rust. Built over [crossterm] with seamless
[clap] derive integration, this library focuses on providing effortless control over custom features. repl-oxide provides some
unique features that allow for creating customized key event processors. See full feature list below.

This library was originally developed for [MatchWire][match_wire], which required a REPL interface capable of handling both
blocking commands and asynchronous background tasks that could still print messages cleanly. Additionally, the project needed a
flexible interface for defining persistent "modes"—referred to as _input hooks_—allowing complete control over how input is 
processed and how state is displayed. Since MatchWire runs on a single-threaded Tokio runtime, all of this had to function
smoothly without relying on multithreading. Existing libraries also lacked smaller but important features, such as discarding
buffered key inputs during blocking operations and the importing/exporting of user commands.

## Install
Add repl-oxide as a dependency to your Cargo.toml with:
```toml
repl-oxide = { git = "https://github.com/WardLordRuby/repl-oxide" }
```
Note: Until the first stable release breaking changes to the public API may occur. The current interface is usable but subject
to refinement.

### Feature Flags
| Feature         | Includes | Additional Dependency       | Description                                                                            |
| --------------- | -------- | --------------------------- | -------------------------------------------------------------------------------------- |
| `clap`          | Default  | `clap/std` + `clap/color`   | Collection of helpers to easily interact with clap                                     |
| `macros`        | -        | `tracing/attributes`        | Provides a macro to easily import the default event processor for custom REPLs         |
| `runner`        | `macros` | -                           | Adds `.run` method on `Repl` that can be used to quickly start the repl's execution    |
| `spawner`       | `runner` | `tokio/macros` + `tokio/rt` | Adds `.spawn` method on `Repl` that spawns the repl's execution on your tokio runtime  |

## Showcase

<div align="center">
  <h3>Completion example</h3>
  <a href="examples/completion.rs"><img src="https://github.com/user-attachments/assets/81abf67f-60d3-49f6-8375-74f33eb1561e" title="cargo r --example completion --features='runner'" width="85%"/></a>
  <h3>Spawner example</h3>
  <a href="examples/spawner.rs"><img src="https://github.com/user-attachments/assets/224d4258-4173-4879-a9b7-2cef88133b45" title="cargo r --example spawner --features='spawner'" width="85%"/></a>
</div>

### Current Features
- Tab autocompletion: walk forward <kbd>Tab</kbd> and backward <kbd>Shift</kbd> + <kbd>Tab</kbd> through **valid** commands.
- Predictive ghost text: previews previous matching commands and then the most relevant autocompletion suggestion.
- Ghost text completion: complete visible previous commands with the right arrow <kbd>→</kbd>.
- Navigate previous commands with up and down arrows <kbd>↑</kbd>, <kbd>↓</kbd>.
- Colored line styling (opt-out by default): highlights commands, arguments, quoted strings, and errors (e.g., mismatched quotes,
  missing requirements, invalid arguments, commands, or values). Inspired by PowerShell.
- User defined parsing rules and ability to opt-out of auto applied `--help` arguments.
- Customizable prompt and prompt separator.
- Buffered key inputs are discarded during a commands execution.
- Clear the current line with <kbd>Ctrl</kbd> + <kbd>C</kbd>.
- Quit shortcuts, <kbd>Ctrl</kbd> + <kbd>D</kbd> or <kbd>Ctrl</kbd> + <kbd>C</kbd> when the input line is empty.
- Define a custom quit command (e.g., command triggered by <kbd>Ctrl</kbd> + <kbd>D</kbd> or <kbd>Ctrl</kbd> + <kbd>C</kbd>).
- Import/Export command history.
- Tag history entries, filter exports via tag api.
- Dynamic input hooks with async support for precise control over input events.
- Tag input hooks, force remove via tag api.
- Cross-platform support: works on all platforms that crossterm supports.
- Multi-line command and resize support.

## Path to Release
#### TODOs before a crates.io release
- Movable cursor support (e.g., left/right arrow navigation, single char at minimum and word jumps).
- Completion overhaul:
  - Current implementation is quite complex; consider a full rework.
  - Still needs proper support for recursive sub-commands, currently subcommands can only go one layer down by
    masking as a static 'Value'.
  - All completion entries have a global namespace, this limitation does not allow for different commands to
    have the same arguments(short/long). 
  - proc-macro derive would make UX very easy.
- Tests:
  - Integration tests for multi-line text rendering.
  - State tracking tests for line completion (if current implementation persists).
- Set up CI/CD pipeline.

### Contributions
Contributions are welcomed, especially since this project has been moved to the back-burner and is not my highest priority. Feel
free to submit PRs for new features or tackling TODOs. Please keep the public interface well-documented—it’s already in good
shape.

## Build Documentation
Building documentation requires the nightly rust toolchain to be installed.
```
git clone https://github.com/WardLordRuby/repl-oxide.git
cd .\repl-oxide
$env:RUSTDOCFLAGS="--cfg docsrs"
cargo +nightly doc --no-deps --all-features
```
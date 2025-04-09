[match_wire]: https://github.com/WardLordRuby/match_wire
[crossterm]: https://crates.io/crates/crossterm
[clap]: https://crates.io/crates/clap

# repl-oxide
repl-oxide is a work-in-progress, async-first REPL (Read-Eval-Print Loop) library for Rust. Built over [crossterm] with seamless
[clap] derive integration, this library focuses on providing effortless control over custom features. repl-oxide was originally
created for [MatchWire][match_wire]. repl-oxide provides some unique features that allow for creating customized key event
processors. See full feature list below.

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
- Tab autocompletion: walk forward (⇥) and backward (⇧ + ⇥) through **valid** commands.
- Predictive ghost text: previews previous matching commands and then the most relevant autocompletion suggestion.
- Colored line styling (opt-out by default): highlights commands, arguments, quoted strings, and errors (e.g., mismatched quotes,
  missing requirements, invalid arguments, commands, or values). Inspired by PowerShell.
- Customizable prompt and prompt separator.
- Clear the current line with Ctrl+C.
- Quit shortcut with Ctrl+D or Ctrl+C when the input line is empty.
- Embed a custom quit command (e.g., triggered by Ctrl+D/Ctrl+C).
- Import/Export command history.
- Navigate previous commands with ↑ and ↓.
- Dynamic input hooks with async support for precise control over input events.
- Cross-platform support: compiles directly for Windows and Unix targets.
- Multi-line command support and resize support.

## Path to Release
#### TODOs before a crates.io release
- Movable cursor support (e.g., left/right arrow navigation, single char at minimum and word jumps).
- Completion overhaul: revamp public interface with proc-macro derive or a builder pattern; consider a full rework.
- Tests:
  - Integration tests for multi-line text rendering.
  - State tracking tests for line completion (if current implementation persists).
- Set up CI/CD pipeline.

### Contributions
Contributions are welcomed, especially since this project has been moved to the back-burner and is not my highest priority. Feel
free to submit PRs for new features or tackling TODOs. Please keep the public interface well-documented—it’s already in good
shape.
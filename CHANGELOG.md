# Changelog

## 0.1.0

The first release, generated from the exe.dev server's commands as of
2026-11-23.

- `Client` (token from `EXE_TOKEN` by default), with a method for every
  exe.dev command, typed results, and `exec()` for any command line.
- `new_vm()` resolves once the VM runs commands (or its setup script has
  finished), and deletes it if waiting fails or the future is dropped.
- `Vm::run()` with streaming output callbacks, timeouts, and output limits;
  `spawn()` for background processes; `write_file`, `read_file`, `upload`,
  and `download`.
- `sign_token()` signs tokens locally with `ssh-keygen`.
- `Client::builder().user_agent("t3exe/0.1.1")` names a tool built on the
  SDK: requests send `User-Agent: t3exe/0.1.1 exedev-rust/0.1.0`.
- The `blocking` feature: `.block_on()` instead of `.await`.

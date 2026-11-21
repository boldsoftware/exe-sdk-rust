# exedev

The official Rust SDK for [exe.dev](https://exe.dev): create VMs, run
commands on them, and manage your account.

```sh
cargo add exedev
cargo add tokio --features full
```

## Quick start

```rust
#[tokio::main]
async fn main() -> exedev::Result<()> {
    let exe = exedev::Client::from_env()?; // reads EXE_TOKEN

    let vm = exe.new_vm().await?;
    println!("{} {}", vm.name(), vm.https_url());
    let result = vm.run("uname -a").check(true).await?;
    print!("{}", result.stdout_lossy());
    vm.delete().await?;
    Ok(())
}
```

Methods return request builders: set options with their methods, then
`.await` them; a builder does nothing until it is awaited. `new_vm()`
resolves once the VM runs commands, so the first `run()` needs no retry loop,
and if waiting fails, or the future is dropped on the way, it deletes the VM
rather than leak it. Create a token with

```sh
ssh exe.dev ssh-key generate-api-key --label=my-app --exp=30d '--cmds="new,ls,ssh,rm"'
```

Its permissions decide which methods work: `new_vm` waits by running a
command over `ssh`, and `delete` needs `rm`, neither of which a token has by
default. For automation, prefer a key scoped to a tag (`--tag=ci`): it
manages only the VMs with that tag, and VMs it creates get the tag.

The SDK uses Tokio. For synchronous code, enable the `blocking` feature and
call `.block_on()` instead of `.await`:

```rust
use exedev::blocking::BlockOn as _;

let exe = exedev::Client::from_env()?;
for vm in exe.vms().block_on()? {
    println!("{} {}", vm.name(), vm.status());
}
```

## Running commands

`vm.run()` runs a command once, in the VM user's login shell, and returns a
`CommandResult` with its `exit_code`, and `stdout` and `stderr` as bytes
(`stdout_lossy()` for text). A nonzero exit status is not an error unless you
ask with `.check(true)`.

```rust
// A string is a shell command line; an array or Vec is argv, quoted for you.
vm.run("ls -la | wc -l").await?;
vm.run(["python3", "-c", "print('hello')"]).await?;

let r = vm
    .run("make test")
    .cwd("src/app")
    .env("CI", "1")
    .timeout(Duration::from_secs(600))
    .on_stdout(|line| println!("{line}"))   // as it arrives
    .capture(false)                         // stream only; keep nothing
    .await?;
```

Output is capped at 8 MiB per run (`.max_output()` changes it); a run that
exceeds it, times out, or loses its connection is
`Error::Interrupted`, with the output that arrived. Dropping a run's future
ends the command on the VM, and everything it started. A command line longer
than a request can carry is uploaded and run from a file.

## Background processes

`vm.spawn()` starts a command that keeps running after the connection, and
your program, ends:

```rust
let server = vm.spawn("python3 -m http.server 8000").name("web").cwd("site").await?;
let again = vm.process("web").await?;   // from anywhere, later
let logs = again.logs().tail(20).await?;
again.terminate().await?;
let done = server.clone().wait().timeout(Duration::from_secs(10)).await?;
```

`wait()` resolves with the process's `CommandResult`; `poll()` refreshes its
state; `send_signal()`, `terminate()` and `kill()` signal its process group.

## Files

```rust
vm.write_file("app/config.json", br#"{"debug": true}"#.to_vec()).mode(0o600).await?;
let data: Vec<u8> = vm.read_file("app/config.json").await?;
vm.upload("./site", "site").exclude([".git", "*.log"]).await?;
vm.download("logs", "./logs").await?;
```

Relative paths are relative to the VM user's home directory. Reading a file
that does not exist is `Error::FileNotFound`.

## Creating VMs

```rust
let vm = exe
    .new_vm()
    .name("build")                          // default: a generated name
    .image("ghcr.io/me/ci:latest")
    .cpu(4).memory("8G").disk("50G")
    .env("MODE", "ci")
    .tags(["ci"])
    .setup_script("#!/bin/sh\napt-get install -y jq")
    .wait_until(WaitUntil::Setup)           // Ssh (default), Setup, or Created
    .await?;
```

`WaitUntil::Ssh` resolves once the VM runs commands; `Setup` also waits for
its setup script, failing with `Error::VmNotReady` (whose `setup` has the
script's exit code and output) if it fails; `Created` resolves at once. If
the VM is not ready within `.timeout()` (60 seconds, or 10 minutes for
setup), it is `Error::VmNotReady`, and the VM is deleted, unless
`.keep_on_failure(true)` keeps it, as the error's `vm`. New VMs do not email
you unless you ask with `.email(true)`.

## Working with VMs

```rust
for vm in exe.vms().await? { /* or exe.vms().pattern("web-*") */ }
let mut vm = exe.vm("build").await?;         // is_not_found() if there is none

let copy = vm.copy().name("build-2").cpu(8).await?;
vm.restart().await?;                         // waits for SSH again, like new_vm()
vm.pause().await?;
vm.resume().await?;
vm.rename("builder").await?;                 // name(), ssh_dest(), https_url() follow
vm.tag(["ci", "linux"]).await?;
vm.untag(["linux"]).await?;
vm.set_comment("nightly builds").await?;
vm.set_egress(EgressPolicy::Integrations).await?;
vm.resize().memory("16G").restart(true).await?;
vm.url(Some(3000), "/api")?;                 // https://builder.exe.xyz:3000/api
vm.stats().range("7d").await?;
vm.delete().await?;

exe.delete(["a", "b"]).await?;               // several at once; reports failures per VM
```

A `Vm`'s getters are a snapshot: the SDK's own operations keep it current
(which is why they take `&mut self`), and `refresh()` fetches the rest.
`copy()`, `pause()`, and `restart()` first flush the VM's recent writes to its
disk, so that copies and the restarted VM have them; `.flush(false)` skips it.

VM command groups are namespaces on the VM:

```rust
vm.share().port().port(3000).await?;
vm.share().set_public().await?;
vm.share().add("friend@example.com").message("have a look").await?;
vm.share().add_link().await?;
vm.domain().add("app.example.com").await?;
vm.shelley().install().await?;
```

## Your account

Every other exe.dev command is a method, grouped as on the command line:
`ssh exe.dev team members` is `exe.team().members()`, required arguments
are arguments, and flags are builder methods (`--copy-tags=false` is
`.copy_tags(false)`).

```rust
exe.whoami().await?.email;
exe.ssh_key().list().await?;
let key = exe.ssh_key().generate_api_key().label("ci").tag("ci").exp("30d").await?;
exe.ssh_key().generate_api_key().cmds(["ls", "share show"]).await?; // lists are lists
exe.integrations().attach("api", "tag:ci").await?;
exe.team().await?;                         // a group that is also a command can be awaited
exe.team().settings().vm_placement().pool("shared").await?;
exe.billing().credits().await?;
```

Each method's documentation is the command's help, with its CLI usage. For a
command with no method yet, `exe.exec("...")` runs a command line as typed
after `ssh exe.dev` and returns its JSON result (or its text, as a JSON
string); quote values in it with `exedev::quote`.

`exedev::sign_token()` makes a token locally, with no request, by signing
with an SSH key on your account (`ssh exe.dev ssh-key add`), through
`ssh-keygen -Y sign`, so ssh-agent and passphrases work:

```rust
let token = exedev::sign_token("/home/me/.ssh/exe_dev_api")
    .cmds(["new", "ls", "ssh", "rm"])
    .expires_in(Duration::from_secs(3600))
    .sign()?;
```

## Results

Methods whose commands report JSON in a known shape return it as a type from
`exedev::types` (for example `whoami()` returns
`exedev::types::WhoamiResponse`). They are `#[non_exhaustive]`, as exe.dev
may add fields; enums have an `Other` variant for values the SDK does not
know. Commands that report something else return a `serde_json::Value`.

## Errors

Every error is an `exedev::Error`:

- `Api`: exe.dev refused or failed a command; it has the `status`,
  `operation`, response `body`, and a `kind()`: `Authentication` (401),
  `PermissionDenied` (403), `NotFound` (404, also for `run()` on a VM that
  does not exist), `RateLimited` (429, after the client's `max_retries`),
  `VmNotRunning` (`run()` on a paused or stopped VM), or `Other`.
- `Network`: the request did not complete; whether it took effect is unknown.
- `Protocol`: a response was not what the SDK expected.
- `Command`: `run().check(true)` and the command failed.
- `Interrupted`: the command's exit status is unknown: its `kind` is
  `Disconnected`, `OutputLimit`, or `Timeout`.
- `VmNotReady`: a VM did not become ready.
- `ProcessLost`, `NoSuchProcess`, `ProcessTimeout`: from spawned processes.
- `FileNotFound`, `IsADirectory`: from file methods.
- `InvalidArgument`, `Config`, `Io`.

`is_not_found()`, `is_permission_denied()`, `is_timeout()` and `status()`
answer the usual questions. Error messages include the server's reason,
sanitized and shortened, but never your token, the command line, or its
output.

## Configuration

```rust
let exe = exedev::Client::builder()
    .token("exe1.…")                        // default: EXE_TOKEN
    .base_url("https://exe.dev")
    .http_client(reqwest::Client::builder().proxy(proxy).build()?)
    .max_retries(5)                         // retries of rate-limited requests, with backoff
    .user_agent("my-tool/1.2")              // User-Agent: my-tool/1.2 exedev-rust/0.1.0
    .build()?;
```

If you build a tool on the SDK, name it with `.user_agent()`, a product
token like `"my-tool/1.2"`: requests then say they came from your tool,
through this SDK. Without it, the User-Agent is the SDK's alone.

Pass your own `reqwest::Client` for proxies, custom TLS, or tests. A `Client`
is cheap to clone, and can be shared between tasks.

## Development

This SDK is generated from the command tree of exe.dev's server, so it
follows the commands exe.dev offers. `src/` is a copy of `sdk/rust/src` in
the exe.dev repository (generated by `cmd/gensdkrs`); do not edit it here.
To update it:

```sh
scripts/sync.sh ../exe
```

Run the tests with `cargo test --all-features`. They include an integration
test that creates VMs on exe.dev, with a token from `EXE_TOKEN` or
`EXE_TOKEN_FILE` (default `~/.config/exedev/sdk-test-token`); without one,
or with `EXE_SKIP_INTEGRATION=1`, it does nothing. `EXE_SKIP_SSH_INTEGRATION=1`
skips what needs `ssh`.

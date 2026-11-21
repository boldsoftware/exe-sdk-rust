//! Moves files to and from a VM, and runs a server on it that outlives the
//! connection.
//!
//! EXE_TOKEN=... cargo run --example files

use std::time::Duration;

#[tokio::main]
async fn main() -> exedev::Result<()> {
    let exe = exedev::Client::from_env()?;
    let vm = exe.new_vm().await?;
    let result = run(&vm).await;
    vm.delete().await?;
    result
}

async fn run(vm: &exedev::Vm) -> exedev::Result<()> {
    vm.write_file("site/index.html", "<h1>hello from exe.dev</h1>\n")
        .await?;
    let page = vm.read_file("site/index.html").await?;
    println!("wrote {} bytes", page.len());

    // A process that keeps running after this program exits.
    let server = vm.spawn("python3 -m http.server 8000").cwd("site").name("web").await?;
    println!("started {} (pid {:?})", server.name(), server.pid());
    tokio::time::sleep(Duration::from_secs(1)).await;
    let local = vm.run("curl -s localhost:8000").check(true).await?;
    print!("{}", local.stdout_lossy());
    println!("private URL: {}", vm.url(Some(8000), "/")?);

    server.terminate().await?;
    let logs = server.logs().await?;
    print!("{}", String::from_utf8_lossy(&logs.stderr));
    Ok(())
}

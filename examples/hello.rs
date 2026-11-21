//! Creates a VM, runs a command on it, and deletes it.
//!
//! EXE_TOKEN=... cargo run --example hello

#[tokio::main]
async fn main() -> exedev::Result<()> {
    let exe = exedev::Client::from_env()?;
    println!("hello, {}", exe.whoami().await?.email);

    let vm = exe.new_vm().await?;
    println!("created {} ({})", vm.name(), vm.https_url());
    let result = async {
        let result = vm.run("uname -a && nproc").check(true).await?;
        print!("{}", result.stdout_lossy());
        Ok::<_, exedev::Error>(())
    }
    .await;
    vm.delete().await?;
    result
}

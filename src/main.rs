//! The `cheapseek` executable: it binds a loopback port, warms the chat bot,
//! prints the base URL of the API on one line, and serves until Ctrl-C.

use std::io::{self, Write as _};
use std::net::{Ipv4Addr, SocketAddr};
use std::process::ExitCode;

use clap::Parser;
use tokio::net::TcpListener;

/// The stack size of every runtime thread, the bot's included.
///
/// RustBot's own command line runs each turn on the main thread, which has
/// an 8 MiB stack on Linux and macOS. The runtime's threads default to a
/// quarter of that, so they are given the size the bot is known to run in.
const THREAD_STACK_BYTES: usize = 8 * 1024 * 1024;

/// A test server that speaks the OpenAI chat-completions API and answers by
/// rule, with zero inference.
#[derive(Debug, Parser)]
#[command(version)]
struct Args {
    /// The port to listen on, on 127.0.0.1. 0 takes a free port.
    #[arg(long, value_name = "N", default_value_t = 0)]
    port: u16,
}

fn main() -> ExitCode {
    let args = Args::parse();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("cheapseek: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Binds the port, warms the bot, prints the base URL, and serves until
/// Ctrl-C.
fn run(args: &Args) -> io::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(THREAD_STACK_BYTES)
        .build()?;
    runtime.block_on(async {
        // The port is bound first, so a port that is taken fails at once
        // and not after the seconds the bot takes to warm.
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, args.port));
        let listener = TcpListener::bind(address).await?;
        cheapseek::warm_up().await;
        print_base_url(listener.local_addr()?)?;
        cheapseek::serve(listener, ctrl_c()).await
    })
}

/// Prints the one line a caller reads to find the server: the base URL that
/// an OpenAI client takes.
fn print_base_url(address: SocketAddr) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "http://{address}/v1")?;
    stdout.flush()
}

/// Completes when the process receives Ctrl-C.
async fn ctrl_c() {
    if tokio::signal::ctrl_c().await.is_err() {
        // The handler could not be installed, so no Ctrl-C can arrive. The
        // server then runs until the process is killed.
        std::future::pending::<()>().await;
    }
}

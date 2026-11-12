//! `quiddity-node`: a bare-bones command line for a graph on disk, and the
//! headless sync node.

use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand};
use clap_verbosity_flag::{Verbosity, WarnLevel};
use iroh::{Endpoint, EndpointAddr, EndpointId};
use quiddity_core::files::{export_dir, import_dir};
use quiddity_core::{Index, merge_duplicates};
use quiddity_node::blobs::Blobs;
use quiddity_node::gc;
use quiddity_node::store::Store;
use quiddity_node::{Allowed, NetError, Peer, allow_device};
use tracing::{debug, error, warn};

#[derive(Parser, Debug)]
#[command(version, about)]
struct Cli {
    /// The graph's directory, which holds `graph.quiddity`.
    #[arg(long, short, default_value = ".")]
    graph: PathBuf,
    #[command(subcommand)]
    command: Command,
    #[command(flatten)]
    verbosity: Verbosity<WarnLevel>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Create a graph. With --blank, an empty replica to sync an existing
    /// graph into.
    Init {
        #[arg(long)]
        blank: bool,
    },
    /// Import a directory of Markdown: a Quiddity export or a Logseq graph.
    Import { dir: PathBuf },
    /// Export the graph as a directory of Markdown.
    Export { dir: PathBuf },
    /// Write the graph's snapshot, and nothing else, to a file: what a static
    /// site publishes. The graph's own file also holds this device's
    /// endpoint key, so it must never be published.
    Snapshot { file: PathBuf },
    /// Delete the files that nothing in the graph refers to any more. A file
    /// goes only after it has gone unreferenced for the grace period, and
    /// only once every active device has stored a version of the graph from
    /// after the reference went; the first run only notes what is unreferenced.
    /// The node must not be serving: the store allows one process at a time.
    Gc {
        /// Say what would be deleted, and change nothing.
        #[arg(long)]
        dry_run: bool,
        /// How many days a file must have gone unreferenced.
        #[arg(long, default_value_t = 30)]
        grace_days: u32,
    },
    /// Let a device connect to every device holding this graph, by listing
    /// it in the graph. The device's ID is what `id` prints on it.
    Allow { device: EndpointId },
    /// Print this device's ID, for `allow`.
    Id,
    /// List the devices holding this graph, latest first, with when each
    /// last saved it (as far as this device has heard).
    Devices,
    /// Serve the graph to other devices until interrupted.
    Serve {
        /// A device allowed to connect. Repeat for more.
        #[arg(long)]
        allow: Vec<EndpointId>,
        /// Allow any device to connect.
        #[arg(long, conflicts_with = "allow")]
        allow_any: bool,
        /// Write the graph as Markdown to this directory, replacing what is
        /// there, whenever the process receives SIGUSR1: for a backup to take
        /// while the node runs.
        #[arg(long, value_name = "DIR")]
        export: Option<PathBuf>,
        #[command(flatten)]
        net: Net,
    },
    /// Sync with another device until neither has anything more to send.
    Sync {
        peer: EndpointId,
        /// The peer's address, if it can't be found by its ID.
        #[arg(long)]
        addr: Option<SocketAddr>,
        #[command(flatten)]
        net: Net,
    },
}

#[derive(clap::Args, Debug)]
struct Net {
    /// Listen on this address only, without relays or address lookup:
    /// for a local network, or testing.
    #[arg(long)]
    local: Option<SocketAddr>,
}

/// An error from the network, for `?` in `anyhow` code.
fn net<T>(result: Result<T, NetError>) -> anyhow::Result<T> {
    result.map_err(anyhow::Error::from_boxed)
}

impl Net {
    async fn endpoint(&self, store: &Store) -> anyhow::Result<Endpoint> {
        net(quiddity_node::bind(store.key()?, self.local).await)
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = Cli::parse();
    // What a command reports goes to stdout; the log is for diagnosis, so it
    // goes to stderr, and `-v` and `-q` set how much of it there is.
    tracing_subscriber::fmt()
        .with_max_level(args.verbosity)
        .with_writer(std::io::stderr)
        .init();
    match run(args).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // The command's result, which `-q` must not hide.
            eprintln!("quiddity-node: {e:#}");
            ExitCode::FAILURE
        }
    }
}

/// Replaces `dir` with an export of the graph. It is written beside `dir` and
/// then swapped in, so `dir` is never seen half-written; only the moment
/// between removing the old one and renaming the new one has no `dir`.
fn export_replacing(peer: &Peer, dir: &Path) -> anyhow::Result<usize> {
    let mut staging = dir.as_os_str().to_owned();
    staging.push(".new");
    let staging = PathBuf::from(staging);
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    let written = export_dir(peer.graph(), &staging)?;
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    std::fs::rename(&staging, dir)?;
    Ok(written)
}

/// Waits for Ctrl-C. With `export`, SIGUSR1 meanwhile writes an export there
/// and says so on stdout, which is how a caller that sent the signal knows
/// the export is complete.
#[cfg(unix)]
async fn wait_for_stop(peer: &Peer, export: Option<&Path>) -> anyhow::Result<()> {
    use tokio::signal::unix::{SignalKind, signal};
    let mut usr1 = signal(SignalKind::user_defined1())?;
    loop {
        tokio::select! {
            result = tokio::signal::ctrl_c() => return Ok(result?),
            _ = usr1.recv() => match export {
                Some(dir) => match export_replacing(peer, dir) {
                    Ok(written) => println!("exported {written} entities to {}", dir.display()),
                    Err(e) => error!(error = %e, "export failed"),
                },
                None => warn!("SIGUSR1 ignored: serve was not given --export"),
            },
        }
    }
}

#[cfg(not(unix))]
async fn wait_for_stop(_peer: &Peer, _export: Option<&Path>) -> anyhow::Result<()> {
    Ok(tokio::signal::ctrl_c().await?)
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    debug!(graph = %cli.graph.display(), command = ?cli.command, "running");
    let store = Store::new(cli.graph);
    match cli.command {
        Command::Init { blank } => {
            store.init(blank)?;
            println!("{}", store.key()?.public());
        }
        Command::Import { dir } => {
            let graph = store.load()?;
            if graph.version() == quiddity_core::Version::default() {
                // Edits made before joining rest on nothing a compacted
                // device could apply.
                anyhow::bail!("this is a blank replica: sync it before importing into it");
            }
            let imported = import_dir(&graph, &dir)?;
            merge_duplicates(&graph, &Index::build(&graph)?)?;
            store.save(&graph)?;
            println!("imported {} entities", imported.len());
            if dir.join("assets").is_dir() {
                let blobs = Blobs::open(store.dir()).await?;
                let files = blobs.import_assets(&dir).await?;
                blobs.shutdown().await?;
                print!("imported {} files", files.stored);
                if files.skipped > 0 {
                    print!(", left {} that are not named for their hash", files.skipped);
                }
                println!();
            }
        }
        Command::Export { dir } => {
            let graph = store.load()?;
            let written = export_dir(&graph, &dir)?;
            println!("exported {written} entities");
            if Index::build(&graph)?.assets().next().is_some() {
                let blobs = Blobs::open(store.dir()).await?;
                let files = blobs.export_assets(&graph, &dir).await?;
                blobs.shutdown().await?;
                print!("exported {} files", files.written);
                if files.missing > 0 {
                    print!(", {} not on this device yet", files.missing);
                }
                println!();
            }
        }
        Command::Snapshot { file } => {
            std::fs::write(&file, store.load()?.snapshot()?)?;
            println!("wrote {}", file.display());
        }
        Command::Gc {
            dry_run,
            grace_days,
        } => {
            let graph = store.load()?;
            if graph.version() == quiddity_core::Version::default() {
                anyhow::bail!("this is a blank replica: sync it before collecting its files");
            }
            let grace = jiff::SignedDuration::from_hours(24 * i64::from(grace_days));
            let found =
                gc::collect(store.dir(), &graph, jiff::Timestamp::now(), grace, dry_run).await?;
            // This device has now seen what it judged, which other devices
            // wait for before they delete.
            if !dry_run {
                store.save(&graph)?;
            }
            println!("{} files in use", found.in_use);
            println!("{} unreferenced, not yet due", found.waiting);
            let verb = if dry_run { "would delete" } else { "deleted" };
            println!("{verb} {} files", found.removed.len());
        }
        Command::Allow { device } => {
            let graph = store.load()?;
            allow_device(&graph, &device.to_string(), jiff::Timestamp::now())?;
            store.save(&graph)?;
            println!("{device} may connect");
        }
        Command::Id => println!("{}", store.key()?.public()),
        Command::Devices => {
            let me = store.key()?.public().to_string();
            let mut devices = store.load()?.devices();
            devices.sort_by(|a, b| b.seen.cmp(&a.seen).then_with(|| a.id.cmp(&b.id)));
            for device in devices {
                let this = if device.id == me {
                    " (this device)"
                } else {
                    ""
                };
                println!("{} {}{this}", device.id, device.seen);
            }
        }
        Command::Serve {
            allow,
            allow_any,
            export,
            net,
        } => {
            let me = store.key()?.public().to_string();
            let others = store.load()?.devices().iter().any(|d| d.id != me);
            if allow.is_empty() && !allow_any && !others {
                anyhow::bail!(
                    "no device is allowed to connect: use `allow`, or --allow or --allow-any"
                );
            }
            let allowed = if allow_any {
                Allowed::Any
            } else {
                Allowed::Only(allow.into_iter().collect::<HashSet<_>>())
            };
            let peer = Peer::new(store.load()?, allowed, store.key()?.public().to_string());
            peer.set_blobs(quiddity_node::blobs::Blobs::open(store.dir()).await?);
            let server = self::net(quiddity_node::serve(&peer, store.key()?, net.local).await)?;
            let endpoint = server.endpoint();
            match net.local {
                Some(_) => println!(
                    "serving {} at {}",
                    endpoint.id(),
                    endpoint.bound_sockets()[0]
                ),
                None => println!("serving {}", endpoint.id()),
            }
            quiddity_node::save_on_change(peer.clone(), store.clone());
            wait_for_stop(&peer, export.as_deref()).await?;
            self::net(server.shutdown().await)?;
            store.save(peer.graph())?;
        }
        Command::Sync {
            peer: id,
            addr,
            net,
        } => {
            let peer = Peer::new(
                store.load()?,
                Allowed::Only(HashSet::from([id])),
                store.key()?.public().to_string(),
            );
            let endpoint = net.endpoint(&store).await?;
            let mut target = EndpointAddr::new(id);
            if let Some(addr) = addr {
                target = target.with_ip_addr(addr);
            }
            peer.connect(&endpoint, target, Some(Duration::from_secs(2)))
                .await?;
            let graph = peer.graph();
            peer.tidy()?;
            store.save(graph)?;
            endpoint.close().await;
            println!("synced");
        }
    }
    Ok(())
}

use log::*;
use std::time::Duration;
use structopt::StructOpt;
use tokio::{
    io,
    net::{TcpListener, TcpStream},
    prelude::*,
};

#[derive(StructOpt, Debug, Clone)]
struct Opt {
    /// Source ip address of forwarder
    #[structopt(name = "source")]
    source: std::net::SocketAddr,
    /// Destination ip address of forwarder
    #[structopt(name = "dest")]
    dest: std::net::SocketAddr,
    /// Set TCP_NODELAY option.
    #[structopt(short = "n", long = "nodelay")]
    nodelay: bool,
    /// Set keepalive interval.
    #[structopt(short = "k", long = "keepalive", default_value = "30")]
    keepalive: u64,
}

fn keepalive(secs: u64) -> Option<Duration> {
    match secs {
        0 => None,
        s => Some(Duration::from_secs(s)),
    }
}

fn sockopt(sock: &TcpStream, cfg: &Opt) {
    sock.set_keepalive(keepalive(cfg.keepalive))
        .unwrap_or_else(|e| error!("{}", e));
    sock.set_nodelay(cfg.nodelay)
        .unwrap_or_else(|e| error!("{}", e));
}

fn peer(src: &TcpStream) -> String {
    src.peer_addr()
        .map(|p| p.to_string())
        .unwrap_or_else(|_| "<unknown>".into())
}

fn fwd(src: TcpStream, cfg: Opt) -> impl Future<Item = (), Error = io::Error> {
    sockopt(&src, &cfg);

    TcpStream::connect(&cfg.dest).and_then(move |dst| {
        sockopt(&dst, &cfg);

        let (srd, swr) = src.split();
        let (drd, dwr) = dst.split();

        let up = io::copy(srd, dwr);
        let down = io::copy(drd, swr);

        up.select(down).map(|_| ()).map_err(|(e, _)| e)
    })
}

fn main() {
    env_logger::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cfg = Opt::from_args();

    info!("Starting: {:?}", cfg);

    let fwd = TcpListener::bind(&cfg.source)
        .into_future()
        .and_then(|sock| {
            sock.incoming()
                .map(move |src| {
                    let addr = peer(&src);

                    info!("Connected ({})", addr);

                    fwd(src, cfg.clone()).then(move |res| match res {
                        Ok(_) => Ok(info!("Disconnected ({})", addr)),
                        Err(e) => Ok(error!("Disconnected with error: {} ({})", e, addr)),
                    })
                })
                .buffer_unordered(usize::max_value())
                .for_each(|_| Ok(()))
        })
        .map(|_| info!("Shutdown"))
        .map_err(|e| error!("Shutdown with error: {}", e));

    tokio::run(fwd);
}

use socket2::{Domain, Socket};
use std::{
    env, io, mem,
    net::{self, *},
    num::NonZeroU32,
    os::unix::io::{FromRawFd, IntoRawFd, RawFd},
};
use futures::prelude::*;

fn main() {
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    builder
        .enable_all()
        .worker_threads(2)
        .max_blocking_threads(1);
    let runtime = builder.build().unwrap();
    if env::args().len() > 1 {
        println!("Opening socket with tokio::task::spawn_blocking");
        runtime
            .block_on(bind_a_lot(open_socket_with_blocking))
            .expect("Failed to bind socket");
    } else {
        println!("Opening socket without tokio::task::spawn_blocking");
        runtime
            .block_on(bind_a_lot(open_socket_without_blocking))
            .expect("Failed to bind socket");
    }
}
async fn bind_a_lot<F: Fn(SocketAddr) -> BindResult + Copy>(bind_socket: F) -> io::Result<()> {

    let mut thundering_herd = futures::stream::futures_unordered::FuturesUnordered::new();
    for _  in 0..1024 {
        thundering_herd.push(bind_loop(bind_socket));
    }

    thundering_herd.select_next_some().await
}


async fn bind_loop<F: Fn(SocketAddr) -> BindResult + Copy>(bind_socket: F) -> io::Result<()> {
    loop {
        let socket =
            open_udp_socket(SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 0), bind_socket).await?;
        mem::drop(socket);
    }
    Ok(())
}

type BindResult = Box<dyn std::future::Future<Output = io::Result<RawFd>> + Unpin>;
async fn open_udp_socket<F: Fn(SocketAddr) -> BindResult + Copy>(
    addr: SocketAddr,
    open_socket: F,
) -> io::Result<tokio::net::UdpSocket> {
    let raw_fd = open_socket(addr).await?;

    let std_socket = unsafe { net::UdpSocket::from_raw_fd(raw_fd) };
    tokio::net::UdpSocket::from_std(std_socket)
}

fn open_socket_with_blocking(addr: SocketAddr) -> BindResult {
    Box::new(Box::pin(async move {
        tokio::task::spawn_blocking(move || open_socket(addr))
            .await
            .unwrap()
    }))
}

fn open_socket_without_blocking(addr: SocketAddr) -> BindResult {
    Box::new(Box::pin(async move { open_socket(addr) }))
}

fn open_socket(addr: SocketAddr) -> io::Result<RawFd> {
    let socket = Socket::new(
        Domain::for_address(addr),
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )?;

    socket.set_nonblocking(true)?;
    match best_interface() {
        Some(iface_index) => {
            if let Err(err) = socket.bind_device_by_index(Some(iface_index)) {
                eprintln!("Failed to bind socket: {}", err);
                return Err(err);
            }
        }
        None => {
            eprintln!("Failed to get best interface index");
        }
    };
    Ok(socket.into_raw_fd())
}

fn best_interface() -> Option<NonZeroU32> {
    let best_interface = b"en0\0";
    NonZeroU32::new(unsafe { libc::if_nametoindex(best_interface.as_ptr() as *const _) })
}

/// A failed URL probe owns its non-listening loopback endpoint through every
/// assertion in `probe`. Shared by the library and end-to-end fixtures.
/// §FS-rhei-agents.6.2.1
fn with_mcp_preflight_failure_endpoint(probe: impl FnOnce(u16)) {
    let socket = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::STREAM,
        Some(socket2::Protocol::TCP),
    )
    .expect("create failure endpoint");
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawSocket;
        use windows_sys::Win32::Networking::WinSock::{
            setsockopt, WSAGetLastError, SOL_SOCKET, SO_EXCLUSIVEADDRUSE,
        };

        // Windows permits a reuse-enabled listener to take a bound address
        // unless its owner requests exclusivity before binding. §FS-rhei-agents.6.2.1
        let exclusive: i32 = 1;
        // SAFETY: socket is live; the option points to an initialized i32 for
        // the duration of the call, with its exact size.
        let result = unsafe {
            setsockopt(
                socket.as_raw_socket() as _,
                SOL_SOCKET,
                SO_EXCLUSIVEADDRUSE,
                (&exclusive as *const i32).cast(),
                std::mem::size_of_val(&exclusive) as i32,
            )
        };
        if result != 0 {
            // SAFETY: WSAGetLastError has no preconditions.
            let error = std::io::Error::from_raw_os_error(unsafe { WSAGetLastError() });
            panic!("reserve failure endpoint exclusively: {error}");
        }
    }
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], 0));
    socket.bind(&address.into()).expect("bind failure endpoint");
    let port = socket.local_addr().expect("address").as_socket().expect("TCP address").port();
    probe(port);
    drop(socket);
}

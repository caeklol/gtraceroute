use std::{any::Any, ascii::Char, mem::MaybeUninit, net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr}, pin::Pin, sync::{atomic::{AtomicBool, Ordering}, mpsc::{self, Sender, TryRecvError}, Arc, RwLock}, time::Duration};

use futures::stream::FuturesUnordered;
use nex_packet::Packet;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::{task::JoinHandle, time::{sleep, Instant}};

static PAYLOAD: &str = "silly 60 byte ping payload!! i think `ping` generates these?";

#[derive(Copy, Clone, PartialEq)]
pub enum PingMode {
    UDP,
    TCP,
    ICMP
}
async fn send_probe(ip: IpAddr, ttl: usize, timeout: Duration, mode: PingMode) {
    let mut seed: u32 = ttl as u32;
    seed ^= seed << 13;
    seed ^= seed >> 17;
    seed ^= seed << 5; 
    if seed == 0 {
        seed = 1;
    }

    for _ in 0..5 {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5; 
    }

    let normalized_rand = seed as f32 / 4294967296.0;
    //let rand_ms = (normalized_rand * 5000f32) as u64;
    println!("probe {}, doing work for {}", ttl, rand_ms);
    tokio::time::sleep(Duration::from_millis(rand_ms)).await;
    //println!("probe {} done", ttl);

    //let seq_number = ttl as u16;

    //match mode {
    //    PingMode::ICMP => {
    //        match ip {
    //            IpAddr::V4(ip) => {
    //                let mut socket = IcmpSocket4::new().unwrap();
    //                socket.set_max_hops(ttl.try_into().unwrap());
    //                socket
    //                    .bind("0.0.0.0".parse::<Ipv4Addr>().unwrap())
    //                    .unwrap();

    //                let payload = PAYLOAD.as_bytes();
    //                let packet = Icmpv4Packet::with_echo_request(42, seq_number, payload.to_vec()).unwrap();

    //                socket.set_timeout(Some(timeout));
    //                socket
    //                    .send_to(ip, packet)
    //                    .unwrap();
    //                drop(socket);
    //            },
    //            IpAddr::V6(ip) => {
    //                let mut socket = IcmpSocket6::new().unwrap();
    //                    socket
    //                        .bind(ip)
    //                        .unwrap();
    //            }
    //        }

    //    },
    //    PingMode::UDP => {
    //        let identifier_port: u16 = 33433 + ttl as u16;
    //        unimplemented!()
    //    },
    //    _ => unimplemented!()
    //}
}

async fn process_icmp(target: IpAddr, nodes: &mut Vec<Node>, timeout: Duration) {
    let start = Instant::now();
    loop {
        let socket = match target {
            IpAddr::V4(_) => Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::ICMPV4)),
            IpAddr::V6(_) => Socket::new(Domain::IPV6, Type::RAW, Some(Protocol::ICMPV6))
        }.expect("failed to create raw socket");
        socket.set_read_timeout(Some(timeout)).expect("failed to set ipv6 options");
        let mut buf: Vec<u8> = vec![0; 512];
        let recv_buf =
            unsafe { &mut *(buf.as_mut_slice() as *mut [u8] as *mut [MaybeUninit<u8>]) };

        match socket.recv_from(recv_buf) {
            Ok((bytes_len, addr)) => {
                let recv_time = Instant::now().duration_since(start);
                let buf = &buf[0..bytes_len];
                match target {
                    IpAddr::V4(target) => {
                        let recv_ip = addr.as_socket_ipv4();
                        if let Some(packet) = nex_packet::ipv4::Ipv4Packet::new(buf) {
                            if let Some(icmp_packet) = nex_packet::icmp::IcmpPacket::new(packet.payload()) {
                                match icmp_packet.get_icmp_type() {
                                    nex_packet::icmp::IcmpType::EchoReply => {},
                                    nex_packet::icmp::IcmpType::TimeExceeded => {},
                                    _ => {}
                                };
                                println!("{:?}", recv_ip);
                            }
                        }
                    },
                    IpAddr::V6(target) => {
                    },
                };
                
                
                break;
            },
            Err(_) => {}
        }

        tokio::time::sleep(Duration::from_millis(50)).await;
    }

}

#[derive(Clone)]
pub struct Node {
    ip: IpAddr,
    latency: Duration
}

#[derive(Clone)]
pub struct TraceState {
    nodes: Vec<Node>,
    min_hops: usize // minimum hops to reach destination
}

pub struct TraceHandler {
    callback: Arc<dyn Fn() + Send + Sync + 'static>,
    state: Arc<RwLock<Option<TraceState>>>,
    cancel: Option<Sender<()>>,
    tracing: Arc<AtomicBool>,
    target: Option<IpAddr>,
    max_hops: usize,
    rx_timeout: Duration,
    tx_timeout: Duration,
    handle: Option<JoinHandle<()>>,
    mode: PingMode
}

impl TraceHandler {
    pub fn new<F>(state: Arc<RwLock<Option<TraceState>>>, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        Self {
            tracing: Arc::new(AtomicBool::new(false)),
            target: None,
            cancel: None,
            callback: Arc::new(callback),
            state,
            max_hops: 0,
            rx_timeout: Duration::from_secs(1),
            tx_timeout: Duration::from_secs(1),
            handle: None,
            mode: PingMode::UDP
        }
    }

    // assumes all necessary options are set
    pub fn begin_trace(&mut self) {
        self.tracing.store(true, Ordering::Relaxed);

        let active = Arc::clone(&self.tracing);
        let callback = Arc::clone(&self.callback);
        let state = Arc::clone(&self.state);

        let max_hops = self.max_hops.clone();
        let rx_timeout = self.rx_timeout.clone();
        let tx_timeout = self.tx_timeout.clone();
        let mode = self.mode.clone();
        let target = self.target.unwrap();

        assert!(self.max_hops != 0);
        let future = async move {
            let mut nodes = Vec::new();
            let min_hops = 6; // traceroute default

            loop {
                let mut probes = Vec::new();

                for n in 1..=min_hops {
                    probes.push(tokio::task::spawn(async move {
                        send_probe(target, n, tx_timeout, mode).await
                    }));
                }

                process_icmp(target, &mut nodes, rx_timeout).await;

                if !active.load(Ordering::Relaxed) {
                    break;
                }

                let mut w = state.write().unwrap();
                *w = Some(TraceState {
                    nodes: nodes.to_owned(),
                    min_hops
                });

                (callback)();
            }
        };
        self.handle = Some(tokio::spawn(future));
    }

    pub fn stop_trace(&mut self) {
        self.tracing.store(false, Ordering::Relaxed);
        self.handle.take().expect("join_handle is None").abort();
    }

    pub fn is_tracing(&self) -> bool {
        return self.tracing.load(Ordering::Relaxed);
    }

    pub fn set_target(&mut self, ip: IpAddr) {
        self.target = Some(ip);
    }

    pub fn set_max_hops(&mut self, hops: usize) {
        self.max_hops = hops;
    }

    pub fn set_rx_timeout(&mut self, timeout: Duration) {
        self.rx_timeout = timeout;
    }

    pub fn set_tx_timeout(&mut self, timeout: Duration) {
        self.tx_timeout = timeout;
    }

    pub fn set_mode(&mut self, mode: PingMode) {
        self.mode = mode;
    }
}

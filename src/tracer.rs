use std::{mem::MaybeUninit, net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6}, sync::{atomic::{AtomicBool, Ordering}, Arc}, time::Duration};

use futures::stream::FuturesUnordered;
use nex_packet::{icmp::IcmpType, icmpv6::Icmpv6Type, ipv4::Ipv4Packet, ipv6::Ipv6Packet, Packet};
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use tokio::{sync::{Mutex, RwLock}, task::JoinHandle, time::{sleep, Instant}};

use crate::packet;

static PAYLOAD: &str = "silly 60 byte ping payload!! i think `ping` generates these?";

#[derive(Copy, Clone, PartialEq)]
pub enum PingMode {
    UDP,
    TCP,
    ICMP
}


#[derive(Clone)]
pub struct Node {
    pub ip: IpAddr,
    pub latency: Duration
}

#[derive(Clone)]
pub struct TraceState {
    pub nodes: Vec<Option<Node>>,
    pub min_hops: usize // minimum hops to reach destination
}

pub struct TraceHandler {
    callback: Arc<dyn Fn() + Send + Sync + 'static>,
    state: Arc<RwLock<Option<TraceState>>>,
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
            callback: Arc::new(callback),
            state,
            max_hops: 30,
            rx_timeout: Duration::from_secs(3),
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
            let mut min_hops = 6; // traceroute default
            let mut target_hop = usize::MAX;
            
            

            loop {
                let mut w = state.write().await;
                *w = Some(TraceState {
                    nodes: nodes.clone(),
                    min_hops
                });

                drop(w);

                (callback)();
                //let mut probes = Vec::new();

                for n in 1..=min_hops {
                    //probes.push(tokio::task::spawn(async move {
                        packet::send_probe(target, n, tx_timeout, mode).await.expect("ughh");
                    //}));
                }

                let start = Instant::now();
                let socket = match target {
                    IpAddr::V4(_) => Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::ICMPV4)),
                    IpAddr::V6(_) => Socket::new(Domain::IPV6, Type::RAW, Some(Protocol::ICMPV6))
                }.expect("failed to create raw socket");
                socket.set_read_timeout(Some(Duration::from_millis(100))).expect("failed to set ipv6 options");
                let mut buf: Vec<u8> = vec![0; 512];
                let recv_buf =
                    unsafe { &mut *(buf.as_mut_slice() as *mut [u8] as *mut [MaybeUninit<u8>]) };


                let mut hops_found = 0;
                
                loop {
                    if !active.load(Ordering::Relaxed) {
                        break;
                    }

                    match socket.recv_from(recv_buf) {
                        Ok((bytes_len, _)) => {
                            let buf = &buf[0..bytes_len];
                            if let Some((src, hop, is_target)) = packet::parse_packet(buf, target, mode) {
                                hops_found += 1;

                                if hop > target_hop {
                                    break;
                                }

                                if nodes.len() < hop {
                                    nodes.resize(hop, None);
                                }

                                nodes[hop-1] = Some(Node {
                                    latency: Instant::now().duration_since(start),
                                    ip: src
                                });

                                if is_target {
                                    target_hop = hop;
                                }
                            }

                        },
                        Err(_) => {}
                    }

                    if Instant::now().duration_since(start) > rx_timeout || hops_found == min_hops || hops_found == target_hop {
                        break;
                    }
                }



                if target_hop != usize::MAX {
                    min_hops = target_hop;
                } else {
                    min_hops += 5;
                    if min_hops > max_hops {
                        min_hops = max_hops;
                    }
                }

                if hops_found == target_hop {
                    tokio::time::sleep(Duration::from_secs(2)).await;    // avoid spamming target
                                                                         // after already having
                                                                         // found the route
                }

                w = state.write().await;
                *w = Some(TraceState {
                    nodes: nodes.clone(),
                    min_hops
                });

                drop(w);

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


use std::{mem::MaybeUninit, net::IpAddr, str::FromStr, sync::{atomic::{AtomicBool, Ordering}, Arc}, time::Duration};

use futures::future::join_all;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::{sync::RwLock, task::JoinHandle, time::Instant};

use crate::packet;

#[derive(Copy, Clone, PartialEq)]
pub enum PingMode {
    UDP,
    TCP,
    ICMP
}


#[derive(Clone, Debug)]
pub struct Ping {
    pub ip: IpAddr,
    pub latency: Duration
}

type Iteration = Vec<Option<Ping>>;

#[derive(Clone)]
pub struct TraceState {
    pub iterations: Vec<Iteration>
}

#[derive(Clone, Copy)]
pub struct TraceOpts {
    pub mode: PingMode,
    pub rx_timeout: Duration,
    pub tx_timeout: Duration,
    pub attempts: usize,
    pub target: IpAddr,
    pub max_hops: usize,
}

impl Default for TraceOpts {
    fn default() -> Self {
        return Self {
            target: IpAddr::from_str("1.1.1.1").unwrap(),
            mode: PingMode::UDP,
            max_hops: 30,
            rx_timeout: Duration::from_secs(3),
            tx_timeout: Duration::from_secs(1),
            attempts: 1,
        };
    }
}

pub struct TraceHandler {
    callback: Arc<dyn Fn() + Send + Sync + 'static>,
    state: Arc<RwLock<Option<TraceState>>>,
    tracing: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl TraceHandler {
    pub fn new<F>(state: Arc<RwLock<Option<TraceState>>>, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        Self {
            tracing: Arc::new(AtomicBool::new(false)),
            callback: Arc::new(callback),
            state,
            handle: None,
        }
    }

    pub fn begin_trace(&mut self, opts: TraceOpts) {
        self.tracing.store(true, Ordering::Relaxed);

        let active = Arc::clone(&self.tracing);
        let callback = Arc::clone(&self.callback);
        let state = Arc::clone(&self.state);

        let future = async move {
            let mut iters = Vec::new();

            let mut iter: usize = 0;
            
            loop {
                iters.resize(iter+1, vec![]);
                let mut w = state.write().await;
                *w = Some(TraceState {
                    iterations: iters.clone()
                });

                drop(w);

                (callback)();

                let mut probes = Vec::new();

                for n in 0..opts.max_hops {
                    probes.push(tokio::task::spawn(async move {
                        println!("n: {}", n);
                        let probe_futures = packet::send_probe(opts.target, n, opts.tx_timeout, opts.mode, opts.attempts).await;
                        join_all(probe_futures).await.into_iter().try_fold((), |_, res| res)
                    }).await);
                }

                println!("probes sent");

                let start = Instant::now();
                let socket = match opts.target {
                    IpAddr::V4(_) => Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::ICMPV4)),
                    IpAddr::V6(_) => Socket::new(Domain::IPV6, Type::RAW, Some(Protocol::ICMPV6))
                }.expect("failed to create raw socket");
                socket.set_read_timeout(Some(Duration::from_millis(100))).expect("failed to set ipv6 options");
                let mut buf: Vec<u8> = vec![0; 512];
                let recv_buf =
                    unsafe { &mut *(buf.as_mut_slice() as *mut [u8] as *mut [MaybeUninit<u8>]) };


                let mut hops_discovered = 0;
                let mut iter_hops = Vec::new();
                loop {
                    if !active.load(Ordering::Relaxed) {
                        break;
                    }

                    match socket.recv_from(recv_buf) {
                        Ok((bytes_len, _)) => {
                            let buf = &buf[0..bytes_len];
                            if let Some((src, hop, _)) = packet::parse_packet(buf, opts.target, opts.mode, opts.attempts) {
                                let ping = Ping {
                                    ip: src,
                                    latency: Instant::now().duration_since(start)
                                };

                                if iters[iter].len() <= hop {
                                    iters[iter].resize(hop+1, None);
                                }

                                if iter_hops.iter().position(|x| x==hop).is_none() {
                                    hops_discovered += 1;
                                    iter_hops.push(hop.into());
                                }

                                iters[iter][hop] = Some(ping);
                            }

                        },
                        Err(_) => {}
                    }

                    if Instant::now().duration_since(start) > opts.rx_timeout || hops_discovered == opts.max_hops {
                        break;
                    }
                }

                tokio::time::sleep(Duration::from_millis(100)).await;

                w = state.write().await;
                *w = Some(TraceState {
                    iterations: iters.clone()
                });

                drop(w);

                (callback)();

                iter += 1;
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
}


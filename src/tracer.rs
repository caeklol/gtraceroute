use std::{net::IpAddr, sync::{mpsc::{self, Sender, TryRecvError}, Arc, RwLock}, thread::{self}, time::Duration};

// core of tracert. i could have probably used an external lib yes but this is so much cooler and
// so much more aura

fn do_probe(ip: IpAddr, ttl: usize, timeout: Duration) -> Option<(IpAddr, Duration)> {
    None
}

#[derive(Clone)]
pub struct Node {
    ip: IpAddr,
    latency: Duration
}

#[derive(Clone)]
pub struct TraceState {
    nodes: Vec<Node>,
    ttl: usize,
}

pub struct TraceHandler {
    callback: Arc<dyn Fn() + Send + Sync + 'static>,
    state: Arc<RwLock<Option<TraceState>>>,
    cancel: Option<Sender<()>>,
    tracing: bool,
    ip: Option<IpAddr>,
    max_ttl: usize,
    timeout: Duration
}

impl TraceHandler {
    pub fn new<F>(state: Arc<RwLock<Option<TraceState>>>, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        Self {
            tracing: false,
            ip: None,
            cancel: None,
            callback: Arc::new(callback),
            state,
            max_ttl: 0,
            timeout: Duration::new(0, 0)
        }
    }

    pub fn is_tracing(&self) -> bool {
        return self.tracing;
    }

    pub fn set_ip(&mut self, ip: IpAddr) {
        self.ip = Some(ip);
    }

    pub fn set_max_ttl(&mut self, max_ttl: usize) {
        self.max_ttl = max_ttl;
    }

    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    pub fn begin_trace(&mut self) {
        if let Some(ip) = self.ip.clone() {
            self.tracing = true;

            let (cancel_tx, cancel_rx) = mpsc::channel();
            self.cancel = Some(cancel_tx);

            let callback = Arc::clone(&self.callback);
            let state = Arc::clone(&self.state);
            let max_ttl = self.max_ttl.clone();
            let timeout = self.timeout.clone();
            thread::spawn(move || {
                let mut ttl = 1;
                let mut nodes = Vec::with_capacity(max_ttl);
                loop {
                    match cancel_rx.try_recv() {
                        Ok(_) | Err(TryRecvError::Disconnected) => {
                            println!("recv cancel");
                            break;
                        }
                        Err(TryRecvError::Empty) => {}
                    }

                    if let Some(res) = do_probe(ip, ttl, timeout) {
                        nodes.insert(ttl-1, Node {
                            ip: res.0,
                            latency: res.1
                        });
                    }

                    match cancel_rx.try_recv() {
                        Ok(_) | Err(TryRecvError::Disconnected) => {
                            println!("recv cancel");
                            break;
                        }
                        Err(TryRecvError::Empty) => {}
                    }
                    let mut w = state.write().unwrap();
                    *w = Some(TraceState {
                        nodes: nodes.to_owned(),
                        ttl, 
                    });

                    (callback)();

                    if ttl < max_ttl {
                        ttl += 1; 
                    } else {
                        break;
                    }
                }
            });
        } else {
            panic!("ip is empty when begin trace");
        }
    }

    pub fn stop_trace(&mut self) {
        assert!(self.tracing);
        self.tracing = false;
        self.cancel.take().expect("cancel_rx is None").send(()).expect("could not send cancel signal");
    }
}

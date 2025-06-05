use std::{net::IpAddr, sync::{mpsc::{self, Sender, TryRecvError}, Arc, RwLock}, thread::{self}, time::Duration};

#[derive(Clone, Copy)]
pub struct Node {
    ip: IpAddr,
    latency: Duration,
    n: usize
}

#[derive(Clone)]
pub struct TraceState {
    nodes: Vec<Node>,
    ttl: u32,
}

pub struct TraceHandler {
    tracing: bool,
    ip: Option<IpAddr>,
    cancel: Option<Sender<()>>,
    callback: Arc<dyn Fn() + Send + Sync + 'static>,
    state: Arc<RwLock<Option<TraceState>>>
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
            state
        }
    }

    pub fn is_tracing(&self) -> bool {
        return self.tracing;
    }

    pub fn set_ip(&mut self, ip: IpAddr) {
        self.ip = Some(ip);
    }


    pub fn begin_trace(&mut self) {
        if let Some(ip) = self.ip {
            self.tracing = true;

            let (cancel_tx, cancel_rx) = mpsc::channel();
            self.cancel = Some(cancel_tx);

            let callback = Arc::clone(&self.callback);
            let state = Arc::clone(&self.state);
            thread::spawn(move || {
                loop {
                    match cancel_rx.try_recv() {
                        Ok(_) | Err(TryRecvError::Disconnected) => {
                            println!("recv cancel");
                            break;
                        }
                        Err(TryRecvError::Empty) => {}
                    }

                    (callback)();
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

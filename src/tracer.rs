use std::{net::IpAddr, sync::{mpsc::{self, Sender, TryRecvError}, Arc}, thread::{self}, time::Duration};

#[derive(Clone, Copy, PartialEq)]
pub struct TraceState {
    pub counter: u32
}

impl Default for TraceState {
    fn default() -> Self {
        Self { counter: u32::MAX }
    }
}

pub struct TraceHandler {
    tracing: bool,
    ip: Option<IpAddr>,
    cancel: Option<Sender<()>>,
    callback: Arc<dyn Fn(TraceState) + Send + Sync + 'static>
}

impl TraceHandler {
    pub fn new<F>(callback: F) -> Self
    where
        F: Fn(TraceState) + Send + Sync + 'static,
    {
        Self {
            tracing: false,
            ip: None,
            cancel: None,
            callback: Arc::new(callback)
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

            let callback_clone = Arc::clone(&self.callback);
            thread::spawn(move || {
                let mut state = TraceState {
                    counter: 0
                };

                loop {

                    match cancel_rx.try_recv() {
                        Ok(_) | Err(TryRecvError::Disconnected) => {
                            println!("recv cancel");
                            break;
                        }
                        Err(TryRecvError::Empty) => {}
                    }

                    (callback_clone)(state);
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

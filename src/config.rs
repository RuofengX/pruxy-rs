use std::env;

use anyhow::bail;
use futures_concurrency::stream::StreamGroup;
use log::{debug, info, warn};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;

use crate::controller::{Controller, Proxy};

const ADDR_NAME: &str = "PRUXY_ADDR";
const PORT_NAME: &str = "PRUXY_PORT";
const UPSTREAM_NAME: &str = "PRUXY_UPSTREAM";

pub struct ProxyBuilder {
    /// 下游，多个地址
    addrs: Vec<String>,
    /// 下游，多个端口
    ports: Vec<u16>,
    /// 上游，包含（一个）端口
    target_addr: Option<String>,
}
impl ProxyBuilder {
    pub fn new() -> Self {
        Self {
            addrs: vec![],
            ports: vec![],
            target_addr: None,
        }
    }
    pub fn from_env() -> anyhow::Result<Self> {
        info!("load config from environment variable");
        info!("reading {}", ADDR_NAME);
        let addrs: Vec<String> = env::var(ADDR_NAME)?
            .split(",")
            .map(|s| s.to_string())
            .collect();
        warn!("{}: {:?}", ADDR_NAME, addrs);

        info!("reading {}", PORT_NAME);
        let ports: Vec<u16> = env::var(PORT_NAME)?
            .split(",")
            .map(|s| s.parse())
            .flat_map(|x| match x {
                Ok(p) => Some(p),
                Err(e) => {
                    warn!("parse port num failure {}, ignore", e);
                    None
                }
            })
            .collect();
        warn!("{}: {:?}", PORT_NAME, ports);

        info!("reading {}", UPSTREAM_NAME);
        let target_addr = env::var(UPSTREAM_NAME)?;
        warn!("{}: {}", UPSTREAM_NAME, target_addr);
        let target_addr = Some(target_addr);

        Ok(Self {
            addrs,
            ports,
            target_addr,
        })
    }
    pub fn bind_addr(mut self, addr: &str) -> Self {
        if addr.len() == 0 {
            warn!("zero length address, ignore");
            return self;
        }
        self.addrs.push(addr.to_string());
        self
    }

    pub fn bind_port(mut self, port: u16) -> Self {
        self.ports.push(port);
        self
    }

    pub fn target_addr(mut self, addr: &str) -> Self {
        if addr.len() == 0 {
            warn!("zero length target address, ignore");
            return self;
        }
        self.target_addr = Some(addr.to_string());
        self
    }

    pub async fn build(self) -> anyhow::Result<Controller> {
        if self.addrs.len() == 0 {
            bail!("no bind address provided");
        }
        if self.ports.len() == 0 {
            bail!("no bind port provided");
        }
        if self.target_addr.is_none() {
            bail!("no target address provided");
        }
        let upstream_addr = self.target_addr.unwrap();

        let mut ret = vec![];
        for &port in self.ports.iter() {
            for addr in self.addrs.iter() {
                let mut listerner: StreamGroup<TcpListenerStream> = StreamGroup::new();

                let addr = addr.as_str();
                debug!("bind address {}:{}", addr, port);

                match TcpListener::bind((addr, port)).await {
                    Ok(tl) => {
                        let tl_stream = TcpListenerStream::new(tl);
                        listerner.insert(tl_stream);
                    }
                    Err(e) => {
                        warn!("bind address {}:{} error {}, ignore", addr, port, e)
                    }
                }

                let (proxy_id, proxy) = Proxy::new(listerner, upstream_addr.clone());
                warn!("proxy_{} created, listening at {}:{}", proxy_id, addr, port);
                ret.push(proxy);
            }
        }
        if ret.len() == 0 {
            bail!("all address bind fail");
        }
        let ret = Controller::new(ret);

        Ok(ret)
    }
}

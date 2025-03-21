use std::{
    env,
    pin::{pin, Pin},
    task::{Context, Poll},
};

use anyhow::bail;
use log::{error, info, warn};
use tokio::net::{TcpListener, TcpStream};
use tokio_stream::{adapters::Merge, wrappers::TcpListenerStream, Stream, StreamExt};

const ADDR_V4_NAME: &str = "PRUXY_BIND_V4";
const ADDR_V6_NAME: &str = "PRUXY_BIND_V6";
const UPSTREAM_NAME: &str = "PRUXY_UPSTREAM";

pub enum Listerners {
    OneStack(TcpListenerStream),
    DualStack(Merge<TcpListenerStream, TcpListenerStream>),
}
impl Stream for Listerners {
    type Item = Result<TcpStream, std::io::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let s = self.get_mut();
        match s {
            Self::OneStack(tl) => pin!(tl).poll_next(cx),
            Self::DualStack(merged) => pin!(merged).poll_next(cx),
        }
    }
}

pub struct ProxyBuilder {
    bind_addr_v4: Option<String>,
    bind_addr_v6: Option<String>,
    target_addr: Option<String>,
}
impl ProxyBuilder {
    pub fn new() -> Self {
        Self {
            bind_addr_v4: None,
            bind_addr_v6: None,
            target_addr: None,
        }
    }
    pub fn from_env() -> anyhow::Result<Self> {
        let bind_addr_v4 = env::var(ADDR_V4_NAME).ok();
        let bind_addr_v6 = env::var(ADDR_V6_NAME).ok();
        if bind_addr_v4.is_none() && bind_addr_v6.is_none() {
            bail!(
                "neither environment variable {} nor {} not set",
                ADDR_V4_NAME,
                ADDR_V6_NAME
            );
        }
        let target_addr = Some(
            env::var(UPSTREAM_NAME)
                .map_err(|_| anyhow::anyhow!("environment variable {} not set", UPSTREAM_NAME))?,
        );
        Ok(Self {
            bind_addr_v4,
            bind_addr_v6,
            target_addr,
        })
    }
    pub fn bind_addr_v4(mut self, addr: &str) -> Self {
        if addr.len() == 0 {
            warn!("ignore zero length IPv4 address");
            return self;
        }
        self.bind_addr_v4 = Some(addr.to_string());
        self
    }
    pub fn bind_addr_v6(mut self, addr: &str) -> Self {
        if addr.len() == 0 {
            warn!("ignore zero length IPv6 address");
            return self;
        }
        self.bind_addr_v4 = Some(addr.to_string());
        self
    }
    pub fn target_addr(mut self, addr: &str) -> Self {
        if addr.len() == 0 {
            warn!("ignore zero length target address");
            return self;
        }
        self.target_addr = Some(addr.to_string());
        self
    }

    pub async fn build(self) -> anyhow::Result<Proxy> {
        if self.target_addr.is_none() {
            bail!("no target address provided");
        }
        let target_addr = self.target_addr.unwrap();

        let listerners = match (self.bind_addr_v4, self.bind_addr_v6) {
            (Some(v4), Some(v6)) => {
                let l4 = TcpListener::bind(v4).await?;
                let l6 = TcpListener::bind(v6).await?;
                let merge = TcpListenerStream::new(l4).merge(TcpListenerStream::new(l6));
                Listerners::DualStack(merge)
            }
            (Some(v4), None) => {
                Listerners::OneStack(TcpListenerStream::new(TcpListener::bind(v4).await?))
            }
            (None, Some(v6)) => {
                Listerners::OneStack(TcpListenerStream::new(TcpListener::bind(v6).await?))
            }
            (None, None) => {
                bail!("no address to bind");
            }
        };
        Ok(Proxy {
            listerners,
            target_addr,
        })
    }
}

/// 代理节点
pub struct Proxy {
    listerners: Listerners,
    target_addr: String,
}

impl Proxy {
    pub async fn serve(mut self) -> anyhow::Result<()> {
        warn!("ready to serve");
        loop {
            let income = self.listerners.next().await.unwrap();
            match income {
                Ok(income_stream) => match TcpStream::connect(&self.target_addr).await {
                    Ok(target_stream) => {
                        let service = Service {
                            downstream: income_stream,
                            upstream: target_stream,
                        };
                        tokio::spawn(service.exchange_stream());
                    }
                    Err(e) => {
                        error!("connect upstream {} fail: {}", self.target_addr, e)
                    }
                },
                Err(e) => warn!("parse new incoming stream error, {}", e),
            }
        }
    }
}

/// 处理入站的连接
pub struct Service {
    upstream: TcpStream,
    downstream: TcpStream,
}

impl Service {
    pub async fn exchange_stream(mut self) -> anyhow::Result<()> {
        let id = tokio::task::id();
        info!("start proxy {}", id);
        let (down, up) = tokio::io::copy_bidirectional_with_sizes(
            &mut self.upstream,
            &mut self.downstream,
            512,
            512,
        )
        .await?;
        let down = byte_unit::Byte::from_u64(down);
        let up = byte_unit::Byte::from_u64(up);
        info!("proxy {} end, down={:#}, up={:#}", id, down, up);
        Ok(())
    }
}

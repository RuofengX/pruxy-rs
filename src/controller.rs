use std::sync::atomic::{AtomicU16, Ordering};

use futures_concurrency::stream::StreamGroup;
use log::{error, info, warn};
use tokio::{net::TcpStream, task::JoinSet};
use tokio_stream::{wrappers::TcpListenerStream, StreamExt};

use crate::exchange::Exchange;

pub struct Controller {
    inner: Vec<Proxy>,
}
impl Controller {
    pub fn new(inner: Vec<Proxy>) -> Self {
        Self { inner }
    }

    pub async fn serve_all(self) {
        let mut task = JoinSet::new();
        for i in self.inner {
            task.spawn(i.serve());
        }
        task.join_all().await;
    }
}

/// 代理节点，一个端口一个代理
pub struct Proxy {
    proxy_id: u16,
    listerner: StreamGroup<TcpListenerStream>,
    /// 上游
    addr: String,
    /// 上游
    port: u16,
}

pub static PROXY_COUNT: AtomicU16 = AtomicU16::new(0);

impl Proxy {
    pub fn new(listerner: StreamGroup<TcpListenerStream>, addr: String, port: u16) -> (u16, Self) {
        let proxy_id = PROXY_COUNT.fetch_add(1, Ordering::AcqRel);
        (
            proxy_id,
            Self {
                proxy_id,
                listerner,
                addr,
                port,
            },
        )
    }

    pub async fn serve(mut self) -> () {
        info!("proxy_{} | ready to serve", self.proxy_id);
        loop {
            match self.listerner.next().await {
                // 获取所有监听器的入站连接
                Some(income) => match income {
                    // 成功获取入站
                    Ok(downstream) => {
                        // 连接上游地址
                        match TcpStream::connect(self.addr.as_str()).await {
                            // 成功连接上游
                            Ok(upstream) => {
                                let service = Exchange::new(self.proxy_id, downstream, upstream);
                                // 启动流交换
                                tokio::spawn(service.exchange_stream());
                            }
                            // 上游连接失败
                            Err(e) => {
                                error!(
                                    "proxy_{} | connect upstream {}:{} fail, {}, ignore",
                                    self.proxy_id, self.addr, self.port, e
                                )
                            }
                        }
                    }
                    // 获取入站连接时，获取到了错误
                    Err(e) => warn!(
                        "proxy_{} | parse new incoming stream error, {}, ignore",
                        self.proxy_id, e
                    ),
                },
                // 没有监听器能够提供入站连接
                None => {
                    error!("proxy_{} | all income listerner dead, exit", self.proxy_id);
                    break;
                }
            }
        }
    }
}

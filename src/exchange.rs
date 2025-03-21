use log::{error, info};
use tokio::net::TcpStream;

/// 处理入站的连接
pub struct Exchange {
    proxy_id: u16,
    upstream: TcpStream,
    downstream: TcpStream,
}

impl Exchange {
    pub fn new(proxy_id: u16, upstream: TcpStream, downstream: TcpStream) -> Self {
        Self {
            proxy_id,
            upstream,
            downstream,
        }
    }
    pub async fn exchange_stream(mut self) -> () {
        let id = tokio::task::id();
        info!("exchange-{}-{} start", self.proxy_id, id,);
        match tokio::io::copy_bidirectional_with_sizes(
            &mut self.upstream,
            &mut self.downstream,
            512,
            512,
        )
        .await
        {
            Ok((up, down)) => {
                let down = byte_unit::Byte::from_u64(down);
                let up = byte_unit::Byte::from_u64(up);
                info!(
                    "exchange-{}-{} end, down={:#}, up={:#}",
                    self.proxy_id, id, down, up
                );
            }
            Err(e) => {
                error!("exchange-{}-{} end with error, {}", self.proxy_id, id, e)
            }
        }
    }
}

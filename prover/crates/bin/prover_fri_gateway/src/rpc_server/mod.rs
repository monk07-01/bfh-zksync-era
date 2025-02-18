mod processor;

use std::sync::Arc;

use jsonrpsee::server::Server;
use tokio::sync::watch;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_prover_interface::rpc::GatewayRpcServer;

use crate::rpc_server::processor::RpcDataProcessor;

pub struct RpcServer {
    pub(crate) processor: RpcDataProcessor,
    pub(crate) ws_port: u16,
}

impl RpcServer {
    pub fn new(
        ws_port: u16,
        blob_store: Arc<dyn ObjectStore>,
        pool: ConnectionPool<Prover>,
    ) -> Self {
        let processor = RpcDataProcessor::new(pool, blob_store);
        Self { processor, ws_port }
    }

    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let address = format!("127.0.0.1:{}", self.ws_port);
        let server = Server::builder().build(address).await?;
        let handle = server.start(self.processor.into_rpc());
        let close_handle = handle.clone();

        tokio::spawn(async move {
            if stop_receiver.changed().await.is_err() {
                tracing::warn!(
                    "Stop signal sender for JSON-RPC server was dropped \
                     without sending a signal"
                );
            }

            close_handle.stop().ok()
        });

        handle.stopped().await;
        Ok(())
    }
}

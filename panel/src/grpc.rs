// Import generated proto
pub mod node_proto {
    tonic::include_proto!("node");
}

use node_proto::node_service_client::NodeServiceClient;
use tonic::transport::Channel;

pub async fn connect(addr: String) -> Result<NodeServiceClient<Channel>, tonic::transport::Error> {
    // Add http:// schema if missing, as tonic requires it
    let url = if addr.starts_with("http") {
        addr
    } else {
        format!("http://{}", addr)
    };
    NodeServiceClient::connect(url).await
}

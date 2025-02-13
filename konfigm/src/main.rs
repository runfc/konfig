mod manager;
use manager::KonfigManager;

use kube::Client as KubeClient;
use kube::runtime::watcher as kube_watcher;

#[tokio::main]
async fn main() -> Result<(), kube_watcher::Error> {
    env_logger::init();

    let kube_client = KubeClient::try_default().await.unwrap();
    let mgr = KonfigManager::new(kube_client.clone());
    tokio::select! {
	_ = mgr.watcher() => {},
	_ = mgr.controller() => {},

	// handle CTRL^C as gracefully as we can.
	_ = tokio::signal::ctrl_c() => {},
    }
    Ok(())
}

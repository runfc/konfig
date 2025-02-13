mod errors;
mod konfignode;
use konfignode::KNodeMgr;

use log;
use gethostname::gethostname;
use kube::Client as KubeClient;
use kube::runtime::watcher as kube_watcher;

async fn register(me: &KNodeMgr) {
    log::info!("Registering myself ...");

    if let Err(err) = me.register().await {
	panic!("Unable to register myself in the k8s control plane: {}", err);
    }
}

async fn unregister(me: &KNodeMgr) {
    log::info!("Unregistering myself ...");

    if let Err(err) = me.unregister().await {
	log::error!("Unable to unregister myself: {}", err);
    }
}

fn get_node_name(hostname: Option<String>) -> String {
    let hostname = match hostname {
	Some(hostname) => hostname,
	None => {
	    // Get the system hostname, instead.
	    gethostname().to_str()
		.expect("unable to convert hostname to string")
		.to_string()
	}
    };
    hostname
}

/*
 * Program design specification:
 *
 *  1. When it starts it needs to register a node
 *  2. Listen all ConfigSets associated with this node
 *  3. Apply all the configuration accordingly
 *  4. Update node status accordingly.
 */
#[tokio::main]
async fn main() -> Result<(), kube_watcher::Error> {
    env_logger::init();

    let name = get_node_name(None);
    let kube_client = KubeClient::try_default().await.unwrap();

    log::info!("starting konfigd for {}", name);
    let me = KNodeMgr::new(kube_client.clone(), name, 60);

    register(&me).await;
    tokio::select! {
	_ = me.watcher() => {},
	_ = me.controller() => {},

	// handle CTRL^C as gracefully as we can.
	_ = tokio::signal::ctrl_c() => {},
    }
    unregister(&me).await;

    Ok(())
}

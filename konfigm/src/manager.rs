use konfig_api as api;

use futures::StreamExt;
use kube::Api as KubeApi;
use kube::Client as KubeClient;
use kube::Error as KubeError;
use kube::api::ListParams as KubeListParams;
use kube::api::ObjectMeta;
use kube::api::Patch as KubePatch;
use kube::api::PatchParams as KubePatchParams;
use kube::runtime::WatchStreamExt;
use kube::runtime::controller::Action as KubeAction;
use kube::runtime::controller::Controller as KubeController;
use kube::runtime::reflector as kube_reflector;
use kube::runtime::watcher as kube_watcher;
use kube::runtime::watcher::Config as KubeWatcherConfig;
use log;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

/*
 * KonfigManager implementation
 */
#[derive(Clone)]
pub struct KonfigManager {
    konfig_api: KubeApi<api::KonfigSet>,
    knode_api: KubeApi<api::KonfigNode>,
}

#[derive(Clone)]
pub struct KonfigManagerCtx {
    manager: KonfigManager,
}

async fn reconcile(konfigset: Arc<api::KonfigSet>, ctx: Arc<KonfigManagerCtx>) -> Result<KubeAction, KubeError> {

    if let Some(selectors) = &konfigset.spec.selectors {
	let label_selectors = selectors.join(",");
	let params = KubeListParams::default()
	    .match_any()
	    .timeout(60)
	    .labels(&label_selectors);

	for knode in ctx.manager.knode_api.list(&params).await? {
	    let mut configsets_list: Vec<api::ConfigsetRef> = vec![];
	    let knode_name = knode.metadata.name.clone().unwrap();
	    let kfg_name = konfigset.metadata.name.clone().unwrap();
	    let kfg_namespace = konfigset.metadata.namespace.clone().unwrap();
	    log::debug!("Config {}/{} should be applied to KonfigNode {:?}", kfg_namespace, kfg_name, knode_name);


	    /* Skip if it's kfg is already assigned to knode */
	    if let Some(mut knode_configs) = knode.spec.configsets {
		if knode_configs.clone().into_iter().any(|kfg| kfg.references(&kfg_name, &kfg_namespace)) {
		    log::debug!("Skipping KonfigSet '{}/{}' as it already assigned to knode '{}'", kfg_name, kfg_namespace, knode_name);
		    return Ok(KubeAction::requeue(Duration::from_secs(30)));
		}

		// add all current configs reference for the update
		configsets_list.append(&mut knode_configs);
	    }
	    configsets_list.push(api::ConfigsetRef::new(&kfg_name, &kfg_namespace));

	    log::debug!("New list of ConfigsetsRef is about to be: {:?}", configsets_list);

	    let mut metadata = ObjectMeta::default();
	    metadata.name = Some(knode_name.clone());
	    let with_new_konfigset = api::KonfigNode{
		metadata: metadata,
		spec: api::konfignode::KonfigNodeSpec{
		    configsets: Some(configsets_list),
		},
		status: None,
	    };
	    let params = KubePatchParams::apply(&knode_name);
	    let patch = KubePatch::Merge(&with_new_konfigset);
	    if let Err(err) = ctx.manager.knode_api.patch(&knode_name, &params, &patch).await {
		log::error!("Unable to assigned konfigset {}/{} to konfig node '{}', got error: {:?}",
			    kfg_namespace, kfg_name, knode_name, err);
		return Err(err);
	    }
	}
    }

    Ok(KubeAction::requeue(Duration::from_secs(15)))
}

fn error_policy(_knode: Arc<api::KonfigSet>, _error: &KubeError, _ctx: Arc<KonfigManagerCtx>) -> KubeAction {
    KubeAction::requeue(Duration::from_secs(60))
}

impl KonfigManager {

    pub fn watcher(&self) -> impl Future<Output = ()> {
	let (_reader, writer) = kube_reflector::store();

	let watcher = kube_watcher(self.konfig_api.clone(), KubeWatcherConfig::default());
	kube_reflector::reflector(writer, watcher)
	    .default_backoff()
	    .applied_objects()
	    .for_each(|obj| {
		log::debug!("Received an update for {:?}", obj);

		futures::future::ready(())
	    })
    }

    pub fn controller(&self) -> impl Future<Output = ()> {
	let ctx = Arc::new(KonfigManagerCtx{
	    manager: self.clone()
	});

	KubeController::new(self.konfig_api.clone(), KubeWatcherConfig::default())
	    .run(reconcile, error_policy, ctx)
	    .for_each(|reconcile| async move {
		log::debug!("Reconciled finished");

		if let Err(err) = reconcile {
		    log::error!("Failed to reconcile with error {:?}", err);
		}
	    })
    }

    pub fn new(kube_client: KubeClient) -> Self {
	Self{
	    konfig_api: KubeApi::all(kube_client.clone()),
	    knode_api: KubeApi::all(kube_client.clone()),
	}
    }
}

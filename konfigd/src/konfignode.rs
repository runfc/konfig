
use configc::Manager;
use configc;
use crate::errors::Error;
use konfig_api as api;

use futures::StreamExt;
use k8s_openapi::api::core::v1::ConfigMap as KubeConfigMap;
use kube::Api as KubeApi;
use kube::Client as KubeClient;
use kube::Error as KubeError;
use kube::api::DeleteParams as KubeDeleteParams;
use kube::api::Patch as KubePatch;
use kube::api::PatchParams as KubePatchParams;
use kube::api::PostParams as KubePostParams;
use kube::runtime::WatchStreamExt;
use kube::runtime::controller::Action as KubeAction;
use kube::runtime::controller::Controller as KubeController;
use kube::runtime::reflector as kube_reflector;
use kube::runtime::watcher as kube_watcher;
use log;
use std::boxed::Box;
use std::collections::BTreeMap;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

/*
 * KNodeMgr encapsulates kube watcher and controller for managing
 * KonfigNode(Custom Resource) objects in k8s.
 *
 */
#[derive(Clone)]
pub struct KNodeMgr {
    name: String,
    reconcilation_interval: u64,

    kube_client: KubeClient,
    knode_api: KubeApi<api::KonfigNode>,
}

#[derive(Clone)]
struct KnodeManagerCtx {
    knode_mgr: KNodeMgr,
}

/*
 * Read content from file's .content key if it has defined, otherwise returns an empty
 * string.
 *
 * for example:
 *
 *   kind: KonfigSet
 *   metadata: [ ... ]
 *   spec:
 *     configuration:
 *      files:
 *       - source: static://
 *         [ ... ]
 *         content: |
 *           This is the file content that we expecting.
 *
 */
fn read_static_content(file: api::KonfigFile) -> String {
    let content = match file.content {
	Some(content) => content,
	None => String::from(""),
    };

    content
}


/*
 * Read content from file's .content key if it has defined, otherwise returns an empty
 * string.
 *
 * for example:
 *
 *   kind: KonfigSet
 *   metadata: [ ... ]
 *   spec:
 *     configuration:
 *      files:
 *       - source: k8s://configmap
 *         key: content  # the configmap's data key where the content should be read from
 *         [ ... ]
 *
 */
async fn read_content_configmap(file: &api::KonfigFile, ctx: Arc<KnodeManagerCtx>, konfigset_namespace: &str) -> Result<String, Error> {
    let namespace = match &file.namespace {
	Some(ns) => ns.to_string(),
	None => konfigset_namespace.to_string(),
    };
    let configmaps: KubeApi<KubeConfigMap> = KubeApi::namespaced(ctx.knode_mgr.kube_client.clone(), &namespace);

    let name = file.source.replace("k8s://configmap/", "");
    let key = match file.key.clone() {
	Some(key) => key,
	None => {
	    let errmsg = format!("For k8s://configmap object the `.key` field is required, got: {:?}", file);
	    return Err(Error::KonfigError(errmsg));
	}
    };

    let content = match configmaps.get(&name).await {
	Err(_) => {
	    let errmsg = format!("Unable to find Configmap with name: {}/{}", namespace, name);
	    return Err(Error::KonfigError(errmsg));
	},
	Ok(configmap) => {
	    let data = match configmap.data {
		Some(data) => data,
		None => {
		    let errmsg = format!("Expected .data inside configmap {}/{}, but couldn't find one?", namespace, name);
		    return Err(Error::KonfigError(errmsg));
		}
	    };

	    let content = match data.get(&key) {
		Some(content) => content,
		None => {
		    let errmsg = format!("The configmap '{}/{}' does not contain '{}' inside its data", namespace, name, key);
		    return Err(Error::KonfigError(errmsg));
		}
	    };
	    content.to_string()
	},
    };

    Ok(content)
}

async fn file_content_from(file: api::KonfigFile, ctx: Arc<KnodeManagerCtx>, konfigset_namespace: &str) -> Result<String, Error> {
    let content = match file.source.as_str() {
	src if src.starts_with("static://") => read_static_content(file),
	src if src.starts_with("k8s://configmap") => read_content_configmap(&file, ctx.clone(), konfigset_namespace).await?,

	/*
	 * When reaching here, it means none of the k8s:// above
	 * matches.  Therefore, it must a mailformed/unsupported k8s
	 * content object
	 */
	src if src.starts_with("k8s://") => {
	    let errmsg = format!("KonfigFile {:?} is mallformed or unsupported: valid values are: k8s://configmap", file);
	    return Err(Error::KonfigError(errmsg));
	}

	// else
	_ => {
	    let errmsg = format!("file source {} is not supported", file.source);
	    return Err(Error::KonfigError(errmsg));
	}
    };

    Ok(content)
}

async fn drifted_configs(konfigset: &api::KonfigSet, ctx: Arc<KnodeManagerCtx>) -> Vec<Box<dyn configc::Manager + Send>> {
    let mut drifted: Vec<Box<dyn configc::Manager + Send>> = Vec::new();
    let name = konfigset.metadata.name.clone().expect("Unable to read konfigset name");
    let namespace = konfigset.metadata.namespace.clone().expect("Unable to read konfigset namespace");
    let me = ctx.knode_mgr.name.as_str();

    let configs = match &konfigset.spec.configurations {
	Some(cfg) => cfg,
	None => {
	    /* no configuration, we can simply return */
	    return vec![];
	}
    };

    // handle sysctls
    log::debug!("handling sysctls for config: {}", name);
    if let Some(sysctls) = &configs.sysctls {
	for sysctl_opt in sysctls {
	    let sysctl = configc::Sysctl::new(sysctl_opt.name.as_str(), sysctl_opt.value.as_str());
	    log::debug!("Managing sysctl: {:?}", sysctl);

	    match sysctl.has_drifted() {
		Err(err) => {
		    log::error!("Unable to check state of sysctl, error: {}", err);
		},
		Ok(has_drifted) => {
		    if has_drifted {
			drifted.push(Box::new(sysctl));
		    }
		}
	    }
	}
    }

    // handle files
    log::debug!("handling files for config: {}", name);
    if let Some(files) = &configs.files {
	for file_opt in files {
	    let dest = file_opt.destination.as_str();
	    let mode = match file_opt.mode {
		Some(mode) => mode,
		None => 0644,
	    };
	    let content = match file_content_from(file_opt.clone(), ctx.clone(), &namespace).await {
		Ok(content) => content,
		Err(err) => {
		    log::error!("Unable to get file content from {:?}, error: {}", file_opt, err);

		    if let Err(err) = ctx.knode_mgr.patch_status_state(me, api::KonfigNodeState::FAILED, Some(false)).await {
			log::error!("Unable to update node state, {:?}", err);
		    };
		    continue;
		}
	    };
	    let file = configc::File::new(dest, content.as_str(), mode, 0);

	    log::debug!("Managing file: {:?}", file_opt);

	    match file.has_drifted() {
		Err(err) => {
		    log::error!("Unable to check the state of file, error: {}", err);
		},
		Ok(has_drifted) => {
		    if has_drifted {
			drifted.push(Box::new(file));
		    }
		}
	    }
	}
    }

    drifted
}

fn tern<T>(expr: bool, when_true: T, when_false: T) -> T {
    if expr {
	when_true
    } else {
	when_false
    }
}

async fn knode_reconcile(knode: Arc<api::KonfigNode>, ctx: Arc<KnodeManagerCtx>) -> Result<KubeAction, KubeError> {
    let me = knode.metadata.name.clone().unwrap();

    if ctx.knode_mgr.name != me {
	return Ok(ctx.knode_mgr.requeue());
    }

    if let Some(configs) = &knode.spec.configsets {
	for config in configs {
	    let mut errors = 0;
	    let kfg_name = config.name.clone().unwrap();
	    let kfg_namespace = config.namespace.clone().unwrap();

	    let konfigsets = KubeApi::namespaced(ctx.knode_mgr.kube_client.clone(), &kfg_namespace);
	    if let Some(konfigset) = konfigsets.get_opt(&kfg_name).await? {
		log::debug!("Reconciling for {:?}", konfigset);

		let drifted = drifted_configs(&konfigset, ctx.clone()).await;
		if drifted.len() > 0 {
		    log::debug!("Alright, we have some work to do");
		    ctx.knode_mgr.patch_status_state(&me, api::KonfigNodeState::SYNCING, Some(false)).await?;

		    for configc_mgr in drifted_configs(&konfigset, ctx.clone()).await {
			if let Err(err) = configc_mgr.ensure() {
			    errors += 1;

			    log::error!("Failed to apply configuration: {}", err);
			}
		    }
		}

		let state = tern(errors == 0, api::KonfigNodeState::READY, api::KonfigNodeState::FAILED);
		let ready = tern(errors == 0, Some(true), None);
		ctx.knode_mgr.patch_status_state(&me, state, ready).await?;
	    }
	}
    }

    Ok(ctx.knode_mgr.requeue())
}

fn knode_error_policy(_knode: Arc<api::KonfigNode>, _error: &KubeError, ctx: Arc<KnodeManagerCtx>) -> KubeAction {
    ctx.knode_mgr.requeue()
}

impl KNodeMgr {

    /*
     * watcher returns a Future object that watches on updates from this own node.
     */
    pub fn watcher(&self) -> impl Future<Output = ()> {
	let (_reader, writer) = kube_reflector::store();
	let reflector = kube_reflector::reflector(
	    writer,
	    kube_watcher(self.knode_api.clone(), kube_watcher::Config::default()),
	);

	// return the reflector future, to be used by tokio::select!
	reflector.applied_objects()
	    .for_each(|cfg| {
		log::debug!("Received an update for: {:?}", cfg);

		futures::future::ready(())
	    })
    }

    pub fn controller(&self) -> impl Future<Output = ()> {
	let ctx = Arc::new(KnodeManagerCtx{
	    knode_mgr: self.clone(),
	});

	KubeController::new(self.knode_api.clone(), kube_watcher::Config::default())
	    .run(knode_reconcile, knode_error_policy, ctx)
	    .for_each(|reconcile| async move {
		if let Err(err) = reconcile {
		    log::error!("Failed to reconciled with error: {:?}", err);
		}
	    })
    }

    pub async fn patch_status(&self, name: &str, new_status: api::KonfigNodeStatus) -> Result<(), KubeError> {
	if let Some(me) = self.knode_api.get_opt(name).await? {
	    let mut new_me = me.clone();
	    let new_status = Some(new_status);
	    new_me.status = new_status;

	    let opts = KubePatchParams::default();
	    self.knode_api.patch_status(name, &opts, &KubePatch::Merge(new_me)).await?;
	}

	Ok(())
    }

    /*
     * A wrapper from patch_status() that simply updates the KonfigNode state which is common path in the code.
     */
    pub async fn patch_status_state(&self, name: &str, state: api::KonfigNodeState, synced: Option<bool>) -> Result<(), KubeError> {
	if let Some(me) = self.knode_api.get_opt(name).await? {
	    let opts = KubePatchParams::default();

	    let mut new_me = me.clone();
	    let mut new_status = me.status.clone().unwrap();

	    new_status.state = Some(state.to_string());
	    if let Some(sync) = synced {
		new_status.synced = Some(sync);
	    }

	    new_me.status = Some(new_status);
	    self.knode_api.patch_status(name, &opts, &KubePatch::Merge(new_me)).await?;
	}

	Ok(())
    }

    pub fn default_labels(&self) -> BTreeMap<String, String> {
	let mut labels = BTreeMap::new();

	labels.insert(String::from("konfignodes.runfc.br/name"), self.name.clone());
	labels.insert(String::from("konfignodes.runfc.br/managed"), String::from("true"));
	labels
    }

    pub async fn register(&self) -> Result<(), KubeError> {
	let name = self.name.as_str();

	match self.knode_api.get_opt(name).await? {
	    Some(node) => {
		log::warn!("Interesting! I was already here before, so I'm retaking my position on the control plane: {:?}", node);
	    },
	    None => {
		let new = api::konfignode::new(name, self.default_labels());
		let opts = KubePostParams::default();
		self.knode_api.create(&opts, &new).await?;
	    }
	};

	let status = api::KonfigNodeStatus::default();
	if let Err(err) = self.patch_status(name, status).await {
	    log::warn!("Unable to update instance status: {:?}", err);
	}
	Ok(())
    }

    pub async fn unregister(&self) -> Result<(), KubeError> {
	let name = self.name.as_str();

	if let Some(_) = self.knode_api.get_opt(name).await? {
	    if let Err(err) = self.patch_status_state(name, api::KonfigNodeState::LEAVING, None).await {
		log::warn!("Unable to update instance status: {:?}", err);
	    }

	    let opts = KubeDeleteParams{
		dry_run: false,
		grace_period_seconds: Some(0),
		propagation_policy: None,
		preconditions: None,
	    };
	    self.knode_api.delete(name, &opts).await?;
	}
	Ok(())
    }

    pub fn requeue(&self) -> KubeAction {
	return KubeAction::requeue(Duration::from_secs(self.reconcilation_interval));
    }

    pub fn new(kube_client: KubeClient, name: String, interval: u64) -> Self {
	Self{
	    name: name,
	    reconcilation_interval: interval,

	    /* k8s internal references */
	    kube_client: kube_client.clone(),
	    knode_api: KubeApi::all(kube_client.clone()),
	}
    }
}

use kube::api::ObjectMeta;
use kube_derive::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Copy, Clone)]
pub enum KonfigNodeState {
    /*
     * The first step when the node joins the k8s
     * control plane it enter in the syncing state
     */
    STARTING,

    /*
     * when we are doing some work, we need to tell the control
     * plane
     */
    SYNCING,

    /*
     * After successfully applying the configsets, the node entered
     * in synced state
     */
    READY,

    /*
     * If any configset fails to apply
     */
    FAILED,

    /*
     * when we are leaving
     */
    LEAVING,
}

impl ToString for KonfigNodeState {
    fn to_string(&self) -> String {
	match self {
	    KonfigNodeState::STARTING => String::from("starting"),
	    KonfigNodeState::SYNCING => String::from("syncing"),
	    KonfigNodeState::READY => String::from("ready"),
	    KonfigNodeState::FAILED => String::from("failed"),
	    KonfigNodeState::LEAVING => String::from("leaving"),
	}
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConfigsetRef {
    pub namespace: Option<String>,
    pub name: Option<String>,
}

impl ConfigsetRef {

    pub fn names(&self) -> (String, String) {
	let name = self.name.clone().unwrap();
	let namespace = self.namespace.clone().unwrap();

	(name, namespace)
    }

    pub fn references(&self, name: &str, namespace: &str) -> bool {
	let is_name_ref = match &self.name { Some(n) => n == name, None => false };
	let is_namespace_ref = match &self.namespace { Some(n) => n == namespace, None => false };

	is_name_ref && is_namespace_ref
    }

    pub fn new(name: &str, namespace: &str) -> Self {
	Self{
	    name: Some(name.to_string()),
	    namespace: Some(namespace.to_string()),
	}
    }
}

#[derive(CustomResource, Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[kube(group = "runfc.br", version = "v1alpha", kind = "KonfigNode", namespaced)]
#[serde(rename_all = "camelCase")]
#[kube(status = "KonfigNodeStatus")]
pub struct KonfigNodeSpec {

    // a list of ConfigSet URIs with the configuration set that needs to be
    // applied on this hosts
    pub configsets: Option<Vec<ConfigsetRef>>,
}

impl KonfigNode {

    /*
     * Returns a copy of the configsets as Vec<String>
     */
    pub fn konfigsets(&self) -> Vec<ConfigsetRef> {
	let mut configs: Vec<ConfigsetRef> = vec![];

	if let Some(ref kfgs) = self.spec.configsets {
	    for kfg in kfgs {
		configs.push(kfg.clone());
	    }
	}
	configs
    }
}

pub fn new(name: &str, labels: BTreeMap<String, String>) -> KonfigNode {
    let mut metadata = ObjectMeta::default();
    metadata.name = Some(name.to_string());
    if labels.len() > 0 {
	metadata.labels = Some(labels);
    }

    KonfigNode{
	metadata: metadata,
	spec: KonfigNodeSpec{
	    configsets: Some(Vec::new()),
	},
	status: Some(KonfigNodeStatus::default()),
    }
}


#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct KonfigNodeStatus {

    // the state of the konfignode: joining, ready, leaving
    pub state: Option<String>,

    // whether the node are in sync with its proposed configuration?
    pub synced: Option<bool>,

    // when in FAILED state, puts the reason why the configset couldn't
    // be applied
    pub failed_reason: Option<String>,

    // When the object was last updated
    pub last_updated: Option<u64>,
}

impl KonfigNodeStatus {

    /*
     * Return KonfigNodeStatus (k8s subresource) with default and safe initial values.
     */
    pub fn default() -> KonfigNodeStatus {
	KonfigNodeStatus{
	    state: Some(KonfigNodeState::STARTING.to_string()),
	    synced: Some(false),
	    failed_reason: None,
	    last_updated: None,
	}
    }

    pub fn from(state: KonfigNodeState, synced: bool, failed_reason: &str) -> KonfigNodeStatus {
	KonfigNodeStatus{
	    state: Some(state.to_string()),
	    synced: Some(synced),
	    failed_reason: Some(failed_reason.to_string()),
	    last_updated: Some(1738792666),
	}
    }
}

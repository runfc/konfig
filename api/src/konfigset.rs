use kube_derive::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct KonfigSysctl {

    /* The name (aka: path in /proc/sys of the linux system configuration) */
    pub name: String,

    /* The desired value for the configuration */
    pub value: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct KonfigFile {

    pub ensure: Option<String>,

    pub source: String,

    pub destination: String,

    pub mode: Option<u32>,

    pub key: Option<String>,

    pub content: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Configuration {

    pub sysctls: Option<Vec<KonfigSysctl>>,

    pub files: Option<Vec<KonfigFile>>,
}

#[derive(CustomResource, Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[kube(group = "runfc.br", version = "v1alpha", kind = "KonfigSet", namespaced)]
#[serde(rename_all = "camelCase")]
pub struct KonfigSetSpec {

    /*
     * Provides a list of konfig node selectors to apply
     * this configuration for.
     */
    pub selectors: Option<Vec<String>>,

    /*
     * Defines a configuration entries for the selected konfig node(s)
     */
    pub configurations: Option<Configuration>,
}

pub struct KonfigSetStatus {

    // Defines how many konfignodes references this konfigset
    pub references: u32,

    // When the object was last updated
    pub last_updated: u64,
}

/*
 * runfc/api - defines all objects, including the runfc CRDs to be included
 * in different part of the runfc ecosystem
 */

pub mod konfignode;
pub use konfignode::KonfigNode;
pub use konfignode::KonfigNodeState;
pub use konfignode::KonfigNodeStatus;
pub use konfignode::ConfigsetRef;

pub mod konfigset;
pub use konfigset::KonfigSet;
pub use konfigset::KonfigFile;
pub use konfigset::KonfigSysctl;

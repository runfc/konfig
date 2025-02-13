use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {

    /* encapsulate a kube-rust error */
    #[error("kube error: {0}")]
    Kube(#[from] kube::Error),

    #[error("Konfig Error: {0}")]
    KonfigError(String),

    #[error("`{0}`")]
    Other(String),
}

//pub type Result<T, E = Error> = std::result::Result<T, E>;

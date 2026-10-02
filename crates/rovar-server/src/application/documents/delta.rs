use crate::domain::{
    document::MAX_CONTENT_BYTES,
    error::{Error, Result},
};

pub(super) async fn expand(base: Vec<u8>, delta: Vec<u8>) -> Result<Vec<u8>> {
    tokio::task::spawn_blocking(move || {
        let delta: rovar_format::delta::Delta = serde_json::from_slice(&delta)
            .map_err(|_| Error::Invalid("Invalid metadata delta".into()))?;
        let base = rovar_format::delta::Snapshot::from_bytes(&base, MAX_CONTENT_BYTES)
            .map_err(|_| Error::DeltaBase)?;
        if base.hash()? != delta.base {
            return Err(Error::DeltaBase);
        }
        base.apply(&delta, MAX_CONTENT_BYTES)
            .and_then(|snapshot| snapshot.to_bytes(MAX_CONTENT_BYTES))
            .map_err(|_| Error::Invalid("Invalid metadata delta or result".into()))
    })
    .await
    .map_err(anyhow::Error::from)?
}

#[cfg(test)]
mod tests;

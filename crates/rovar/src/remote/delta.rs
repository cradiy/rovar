use super::*;

/// Reconstruct against the immutable confirmed baseline, never the editable
/// local file. The latter is used only to reuse verified media by content hash.
pub(super) async fn receive(
    client: &Client,
    space: &str,
    expected: &Object,
    baseline: Option<baseline::Baseline>,
    local: &Path,
    downloads: &Path,
    executor: &gpui::BackgroundExecutor,
) -> Result<(Object, Vec<u8>)> {
    let base = baseline
        .filter(|base| {
            base.object.id == expected.id
                && base.object.kind == expected.kind
                && !base.object.deleted
                && base.object.revision > 0
                && base.object.revision <= expected.revision
        })
        .and_then(|base| {
            base.transfer
                .map(|snapshot| (base.object.revision, snapshot))
        });
    let route = format!("spaces/{space}/objects/{}/transfer", expected.id);
    let snapshot = if let Some((revision, base)) = base {
        let (base, hash) = executor
            .spawn(async move {
                let hash = base.hash()?;
                Ok::<_, anyhow::Error>((base, hash))
            })
            .await?;
        let response: rovar_api::Transfer = client
            .json(
                "POST",
                &route,
                Some(serde_json::to_value(rovar_api::DownloadBase {
                    revision,
                    hash,
                })?),
            )
            .await?;
        validate(&response.snapshot.object, expected)?;
        let mut snapshot = response.snapshot;
        if matches!(response.encoding, rovar_api::TransferEncoding::Delta) {
            let content = snapshot.content;
            let rebuilt = executor
                .spawn(async move {
                    let bytes = STANDARD.decode(content)?;
                    ensure!(
                        bytes.len() <= rovar_api::MAX_DELTA_BYTES,
                        "Delta exceeds the size limit"
                    );
                    let patch = serde_json::from_slice(&bytes)?;
                    let bytes = base
                        .apply(&patch, rovar_api::MAX_METADATA_BYTES)?
                        .to_bytes(rovar_api::MAX_METADATA_BYTES)?;
                    Ok::<_, anyhow::Error>(STANDARD.encode(bytes))
                })
                .await;
            match rebuilt {
                Ok(content) => snapshot.content = content,
                // No cache writes occur before validation. A single full fetch
                // also repairs a bad patch without hiding a bad full response.
                Err(_) => snapshot = client.json("GET", &route, None).await?,
            }
        }
        snapshot
    } else {
        client.json("GET", &route, None).await?
    };
    validate(&snapshot.object, expected)?;
    let object = snapshot.object.clone();
    let bytes = media::hydrate(client, space, snapshot, local, downloads, executor).await?;
    Ok((object, bytes))
}

fn validate(actual: &Object, expected: &Object) -> Result<()> {
    ensure!(
        actual.id == expected.id
            && actual.kind == expected.kind
            && actual.revision >= expected.revision
            && !actual.deleted,
        "Downloaded document identity or revision changed"
    );
    Ok(())
}

pub(super) fn prepare(
    base: Option<&rovar_format::delta::Snapshot>,
    input: &Save,
    bytes: &[u8],
) -> Option<String> {
    if input.deleted || input.base_revision == 0 || input.kind == Kind::ColorStyle {
        return None;
    }
    let base = base?;
    let next =
        rovar_format::delta::Snapshot::from_bytes(bytes, rovar_api::MAX_METADATA_BYTES).ok()?;
    let delta = serde_json::to_vec(&base.difference(&next).ok()?).ok()?;
    let full_size = next.to_bytes(rovar_api::MAX_METADATA_BYTES).ok()?.len();
    // Small/new documents and large binary changes may be cheaper as snapshots.
    (delta.len() <= rovar_api::MAX_DELTA_BYTES && delta.len().checked_add(256)? < full_size)
        .then(|| STANDARD.encode(delta))
}

pub(super) async fn send(
    client: &Client,
    space: &str,
    id: &str,
    path: &Path,
    pending: PendingSave,
    transfer: Option<Save>,
    executor: &gpui::BackgroundExecutor,
) -> Result<(Object, PendingSave)> {
    let route = format!("spaces/{space}/objects/{id}");
    let has_delta = pending.delta.is_some();
    let (pending, transfer, body) = executor
        .spawn(async move {
            let input = transfer.as_ref().unwrap_or(&pending.input);
            let body = save_body(input, pending.delta.as_deref(), &pending.input.request_id);
            (pending, transfer, body)
        })
        .await;
    if !has_delta {
        return Ok((client.json("PUT", &route, Some(body)).await?, pending));
    }
    let result = client
        .json("PUT", &format!("{route}/delta"), Some(body))
        .await;
    if !result.as_ref().err().is_some_and(|error| {
        error
            .downcast_ref::<HttpError>()
            .is_some_and(HttpError::is_delta_base_mismatch)
    }) {
        return Ok((result?, pending));
    }
    // Only a definite baseline rejection permits replacing the request. A
    // timeout/unknown outcome must replay the identical patch and request ID.
    let path = path.to_owned();
    let (fallback, body) = executor
        .spawn(async move {
            let mut input = pending.input;
            input.request_id = uuid::Uuid::new_v4().to_string();
            let fallback = PendingSave { input, delta: None };
            upload::store_pending(&path, &fallback)?;
            let input = transfer.as_ref().unwrap_or(&fallback.input);
            let body = save_body(input, None, &fallback.input.request_id);
            Ok::<_, anyhow::Error>((fallback, body))
        })
        .await?;
    Ok((client.json("PUT", &route, Some(body)).await?, fallback))
}

// Serialize the selected content directly. Building a temporary Save would
// duplicate the full snapshot (or copy it only to replace it with a patch).
fn save_body(input: &Save, content: Option<&str>, request_id: &str) -> serde_json::Value {
    serde_json::json!({
        "kind": input.kind,
        "title": input.title,
        "base_revision": input.base_revision,
        "request_id": request_id,
        "content": content.unwrap_or(&input.content),
        "media": input.media,
        "deleted": input.deleted,
    })
}

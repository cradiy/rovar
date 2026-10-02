use super::*;

pub(super) fn prepare(
    base: Option<&rovar_format::delta::Snapshot>,
    input: &Save,
) -> Option<String> {
    if input.deleted || input.base_revision == 0 || input.kind == Kind::ColorStyle {
        return None;
    }
    let base = base?;
    let bytes = STANDARD.decode(&input.content).ok()?;
    let next =
        rovar_format::delta::Snapshot::from_bytes(&bytes, rovar_api::MAX_CONTENT_BYTES).ok()?;
    let delta = serde_json::to_vec(&base.difference(&next).ok()?).ok()?;
    let full_size = next.to_bytes(rovar_api::MAX_CONTENT_BYTES).ok()?.len();
    // Small/new documents and large binary changes may be cheaper as snapshots.
    (delta.len().checked_add(256)? < full_size).then(|| STANDARD.encode(delta))
}

pub(super) async fn send(
    client: &Client,
    space: &str,
    id: &str,
    path: &Path,
    pending: &PendingSave,
    mut transfer: Save,
) -> Result<Object> {
    let route = format!("spaces/{space}/objects/{id}");
    let Some(delta) = &pending.delta else {
        return client
            .json("PUT", &route, Some(serde_json::to_value(&transfer)?))
            .await;
    };
    let full = std::mem::replace(&mut transfer.content, delta.clone());
    let result = client
        .json(
            "PUT",
            &format!("{route}/delta"),
            Some(serde_json::to_value(&transfer)?),
        )
        .await;
    if !result.as_ref().err().is_some_and(|error| {
        error
            .downcast_ref::<HttpError>()
            .is_some_and(HttpError::is_delta_base_mismatch)
    }) {
        return result;
    }
    // Only a definite baseline rejection permits replacing the request. A
    // timeout/unknown outcome must replay the identical patch and request ID.
    let mut input = pending.input.clone();
    input.request_id = uuid::Uuid::new_v4().to_string();
    let fallback = PendingSave {
        input,
        media_transfer: pending.media_transfer,
        delta: None,
    };
    write_atomic(path, &serde_json::to_vec(&fallback)?)?;
    transfer.content = full;
    transfer.request_id = fallback.input.request_id;
    client
        .json("PUT", &route, Some(serde_json::to_value(&transfer)?))
        .await
}

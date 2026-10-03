use crate::domain::{
    document::{Document, DocumentKind, DocumentSnapshot, SaveDocument},
    error::{Error, Result},
    identity::Identity,
};
use base64::{Engine, engine::general_purpose::STANDARD};

pub fn identity(value: Identity) -> rovar_api::Identity {
    rovar_api::Identity {
        server_id: value.server_id,
        user_id: value.user_id,
        username: value.username,
        api_version: rovar_api::VERSION,
        spaces: value.spaces.into_iter().map(space).collect(),
        registration: policy(value.registration),
    }
}

pub fn document(value: Document) -> rovar_api::Object {
    rovar_api::Object {
        id: value.id,
        kind: match value.kind {
            DocumentKind::Document => rovar_api::Kind::Document,
            DocumentKind::Component => rovar_api::Kind::Component,
            DocumentKind::ColorStyle => rovar_api::Kind::ColorStyle,
        },
        title: value.title,
        revision: value.revision,
        created: value.created,
        modified: value.modified,
        deleted: value.deleted,
    }
}

pub fn snapshot(value: DocumentSnapshot) -> rovar_api::Snapshot {
    rovar_api::Snapshot {
        object: document(value.document),
        content: STANDARD.encode(value.content),
        media: value
            .media
            .into_iter()
            .map(|m| rovar_api::Media {
                hash: m.hash,
                length: m.length,
            })
            .collect(),
    }
}

pub fn save(id: String, value: rovar_api::Save) -> Result<SaveDocument> {
    let content = STANDARD
        .decode(value.content)
        .map_err(|_| Error::Invalid("Invalid content encoding".into()))?;
    Ok(SaveDocument {
        id,
        kind: match value.kind {
            rovar_api::Kind::Document => DocumentKind::Document,
            rovar_api::Kind::Component => DocumentKind::Component,
            rovar_api::Kind::ColorStyle => DocumentKind::ColorStyle,
        },
        title: value.title,
        base_revision: value.base_revision,
        request_id: value.request_id,
        content,
        media: value
            .media
            .into_iter()
            .map(|m| crate::domain::document::Media {
                hash: m.hash,
                length: m.length,
            })
            .collect(),
        deleted: value.deleted,
    })
}
pub fn space(value: crate::domain::space::Space) -> rovar_api::Space {
    rovar_api::Space {
        id: value.id,
        name: value.name,
        kind: value.kind,
        role: value.role,
    }
}
pub fn policy(value: crate::domain::space::RegistrationPolicy) -> rovar_api::RegistrationPolicy {
    rovar_api::RegistrationPolicy {
        personal: value.personal,
        teams: value.teams,
    }
}

pub mod config;

use crate::{
    application::{Application, auth::AuthService, documents::DocumentService},
    infrastructure::{
        passwords::ArgonPasswords,
        postgres::{self, auth::AuthRepository, documents::DocumentRepository},
        storage::ContentStore,
    },
};
use std::sync::Arc;

/// The only place where application ports are wired to concrete adapters.
pub async fn build(config: &config::Config) -> anyhow::Result<Application> {
    let storage = ContentStore::open(&config.storage.directory)?;
    let (pool, server_id) = postgres::connect(&config.database.url).await?;
    let spaces = Arc::new(postgres::spaces::SpaceRepository::new(pool.clone()));
    let policy = crate::domain::space::RegistrationPolicy {
        personal: config.registration.personal,
        teams: config.registration.teams,
    };
    Ok(Application {
        spaces: Arc::new(crate::application::spaces::SpaceService::new(
            spaces.clone(),
            policy,
        )),
        auth: Arc::new(AuthService::new(
            Arc::new(AuthRepository::new(pool.clone())),
            Arc::new(ArgonPasswords::new()),
            server_id.clone(),
            spaces,
            policy,
        )),
        documents: Arc::new(DocumentService::new(
            Arc::new(DocumentRepository::new(pool)),
            Arc::new(storage),
        )),
        server_id,
    })
}

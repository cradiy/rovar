pub mod auth;
pub mod documents;
pub mod ports;
pub mod spaces;

use auth::AuthService;
use documents::DocumentService;
use std::sync::Arc;

#[derive(Clone)]
pub struct Application {
    pub spaces: Arc<spaces::SpaceService>,
    pub auth: Arc<AuthService>,
    pub documents: Arc<DocumentService>,
    pub server_id: String,
}

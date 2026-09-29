mod auth;
mod documents;
mod mapping;
mod response;
mod spaces;
mod web;

use crate::{application::Application, bootstrap::config::Config};
use salvo::prelude::*;

pub async fn serve(config: Config, app: Application) {
    let router = routes(&config, app);
    println!("Rovar server listening on {}", config.server.bind);
    Server::new(TcpListener::new(config.server.bind).bind().await)
        .serve(router)
        .await;
}

fn routes(config: &Config, app: Application) -> Router {
    Router::new()
        .hoop(Services(app))
        .push(
            Router::with_path("api/v1")
                .hoop(ApiBoundary {
                    origin: config.server.public_origin.clone(),
                })
                .push(Router::with_path("info").get(info))
                .push(Router::with_path("login").post(auth::login))
                .push(Router::with_path("register").post(auth::register))
                .push(
                    Router::new()
                        .hoop(auth::authenticate)
                        .push(Router::with_path("session").get(auth::session))
                        .push(Router::with_path("logout").post(auth::logout))
                        .push(Router::with_path("spaces").get(spaces::list))
                        .push(Router::with_path("teams").post(spaces::create))
                        .push(Router::with_path("teams/join").post(spaces::join))
                        .push(Router::with_path("teams/{space}/invites").post(spaces::invite))
                        .push(Router::with_path("teams/{space}/members").get(spaces::members))
                        .push(
                            Router::with_path("teams/{space}/members/{user}")
                                .delete(spaces::remove),
                        )
                        .push(Router::with_path("spaces/{space}/objects").get(documents::list))
                        .push(
                            Router::with_path("spaces/{space}/objects/{id}")
                                .get(documents::read)
                                .put(documents::save),
                        ),
                ),
        )
        .push(Router::with_path("{**path}").get(web::serve))
}

struct Services(Application);
#[handler]
impl Services {
    async fn handle(&self, depot: &mut Depot) {
        depot.insert_typed(self.0.clone());
    }
}

struct ApiBoundary {
    origin: String,
}
#[handler]
impl ApiBoundary {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let _ = res.add_header("cache-control", "no-store", true);
        let _ = res.add_header("x-content-type-options", "nosniff", true);
        depot.insert("secure_cookie", self.origin.starts_with("https://"));
        if req.method() != salvo::http::Method::GET
            && let Some(origin) = req.headers().get("origin")
            && origin.to_str().ok() != Some(self.origin.as_str())
        {
            res.status_code(StatusCode::FORBIDDEN);
            res.render(Json(rovar_api::ApiError {
                code: "invalid_origin".into(),
                message: "Request origin is not allowed".into(),
            }));
            ctrl.skip_rest();
        }
    }
}

#[handler]
async fn info(depot: &mut Depot, res: &mut Response) {
    let app = depot.get_typed::<Application>().unwrap();
    res.render(Json(rovar_api::ServerInfo {
        server_id: app.server_id.clone(),
        api_version: rovar_api::VERSION,
        registration: mapping::policy(app.auth.registration),
    }));
}

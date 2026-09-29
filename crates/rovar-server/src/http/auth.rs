use super::response;
use crate::{
    application::{Application, auth::SESSION_SECONDS},
    domain::{error::Error, identity::Identity},
};
use rovar_api::Credentials;
use salvo::prelude::*;

pub struct Authenticated {
    pub identity: Identity,
    token: String,
}

#[handler]
pub async fn authenticate(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
    ctrl: &mut FlowCtrl,
) {
    let token = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_owned)
        .or_else(|| req.cookie("rovar_session").map(|v| v.value().to_owned()));
    let app = depot.get_typed::<Application>().unwrap();
    let result = match token {
        Some(token) => app
            .auth
            .authenticate(&token)
            .await
            .map(|identity| Authenticated { identity, token }),
        None => Err(Error::Unauthorized),
    };
    match result {
        Ok(user) => {
            depot.insert_typed(user);
        }
        Err(error) => {
            response::failure(res, error);
            ctrl.skip_rest();
        }
    }
}

#[handler]
pub async fn login(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    req.set_secure_max_size(4096);
    let credentials: Credentials = match req.parse_json().await {
        Ok(value) => value,
        Err(_) => {
            response::failure(res, Error::Invalid("Invalid credentials".into()));
            return;
        }
    };
    let app = depot.get_typed::<Application>().unwrap();
    let result = app
        .auth
        .login(credentials.username, credentials.password)
        .await;
    finish_login(depot, res, result);
}

fn finish_login(
    depot: &Depot,
    res: &mut Response,
    result: crate::domain::error::Result<crate::domain::identity::Session>,
) {
    if let Ok(signed_in) = &result {
        let secure = *depot.get::<bool>("secure_cookie").unwrap();
        let cookie = format!(
            "rovar_session={}; Path=/api; HttpOnly; SameSite=Strict; Max-Age={SESSION_SECONDS}{}",
            signed_in.token,
            if secure { "; Secure" } else { "" }
        );
        let _ = res.add_header("set-cookie", cookie, true);
    }
    response::render(
        res,
        result.map(|signed_in| rovar_api::Login {
            identity: super::mapping::identity(signed_in.identity),
            token: signed_in.token,
        }),
    );
}

#[handler]
pub async fn session(depot: &mut Depot, res: &mut Response) {
    res.render(Json(super::mapping::identity(
        depot.get_typed::<Authenticated>().unwrap().identity.clone(),
    )));
}

#[handler]
pub async fn logout(depot: &mut Depot, res: &mut Response) {
    let app = depot.get_typed::<Application>().unwrap();
    let user = depot.get_typed::<Authenticated>().unwrap();
    let result = app
        .auth
        .logout(&user.token)
        .await
        .map(|_| serde_json::json!({}));
    if result.is_ok() {
        let _ = res.add_header(
            "set-cookie",
            "rovar_session=; Path=/api; HttpOnly; SameSite=Strict; Max-Age=0",
            true,
        );
    }
    response::render(res, result);
}
#[handler]
pub async fn register(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    req.set_secure_max_size(4096);
    let input: rovar_api::Registration = match req.parse_json().await {
        Ok(input) => input,
        Err(_) => {
            response::failure(res, Error::Invalid("Invalid registration".into()));
            return;
        }
    };
    let result = depot
        .get_typed::<Application>()
        .unwrap()
        .auth
        .register(&input.username, &input.password, input.team_name.as_deref())
        .await;
    finish_login(depot, res, result);
}

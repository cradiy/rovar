use salvo::prelude::*;

#[cfg(feature = "embedded-web")]
#[derive(rust_embed::RustEmbed)]
#[folder = "../../dist/web/"]
struct Assets;

#[handler]
pub async fn serve(req: &mut Request, res: &mut Response) {
    #[cfg(feature = "embedded-web")]
    {
        let path = req.uri().path().trim_start_matches('/');
        let path = if path.is_empty() { "index.html" } else { path };
        if let Some(asset) = Assets::get(path) {
            let _ = res.add_header(
                "content-type",
                mime_guess::from_path(path).first_or_octet_stream().as_ref(),
                true,
            );
            let _ = res.add_header("x-content-type-options", "nosniff", true);
            let _ = res.add_header("cache-control", "no-cache", true);
            let _ = res.write_body(asset.data.into_owned());
            return;
        }
    }
    let _ = req;
    res.status_code(StatusCode::NOT_FOUND);
    res.render("Build with nu scripts/server.nu to embed the Web editor.");
}

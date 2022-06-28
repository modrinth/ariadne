use actix_web::guard::GuardContext;

pub const ADMIN_KEY_HEADER: &str = "Modrinth-Admin";
pub fn admin_key_guard(ctx: &GuardContext) -> bool {
    let admin_key = dotenv::var("ARIADNE_ADMIN_KEY").expect("No admin key provided!");

    ctx.head()
        .headers()
        .get(ADMIN_KEY_HEADER)
        .map_or(false, |it| it.as_bytes() == admin_key.as_bytes())
}

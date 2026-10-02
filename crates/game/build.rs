//! Steam's library is loaded only when Steam is first used, so the game still starts on a
//! machine where the library is missing; the multiplayer screen then says Steam is not
//! available instead of the game refusing to open.

fn main() {
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/DELAYLOAD:steam_api64.dll");
        println!("cargo:rustc-link-lib=delayimp");
    }
}

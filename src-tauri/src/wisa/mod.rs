//! WISA account integration.
//!
//! Everything network-facing lives here rather than in the webview: the access
//! token and the refresh token never reach JavaScript, and no request depends
//! on the webview's origin being present in the API's CORS allow list.
//!
//! Three pieces:
//!
//! - [`auth`] — Google sign-in over a loopback redirect (PKCE, `state`), plus
//!   the refresh path that keeps a signed-in user signed in.
//! - [`provision`] — turns an account key into a `WISA Gateway` provider for
//!   each client and switches to it, so the config writers already in this app
//!   do the writing.
//! - [`keychain`] — where the refresh token rests between launches.

pub mod auth;
pub mod client;
pub mod keychain;
pub mod provision;

pub use auth::WisaAuthState;

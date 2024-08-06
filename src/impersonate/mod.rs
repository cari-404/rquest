#![allow(missing_debug_implementations)]

mod chrome;
mod edge;
pub mod extension;
mod okhttp;
mod profile;
mod safari;

use crate::connect::HttpConnector;
use crate::impersonate::extension::{SslConnectExtension, SslExtension};
use antidote::Mutex;
#[cfg(feature = "socks")]
use boring::ssl::Ssl;
use boring::{
    error::ErrorStack,
    ssl::{ConnectConfiguration, SslConnectorBuilder},
};
use hyper_boring::{HttpsConnector, HttpsLayerSettings, SessionCache};
pub(crate) use profile::configure_impersonate;
use profile::ClientProfile;
pub use profile::{Http2Settings, Impersonate, ImpersonateSettings};
use std::sync::Arc;
use tokio::sync::OnceCell;

type Builder = dyn Fn() -> Result<SslConnectorBuilder, ErrorStack> + Send + Sync;

/// Context for impersonating a client.
#[derive(Clone)]
pub(crate) struct ImpersonateContext {
    pub impersonate: Impersonate,
    pub enable_ech_grease: bool,
    pub permute_extensions: bool,
    pub certs_verification: bool,
    pub pre_shared_key: bool,
    pub h2: bool,
}

const DEFAULT_SESSION_CACHE_CAPACITY: usize = 8;

type Session = Arc<Mutex<SessionCache>>;

/// A wrapper around a `SslConnectorBuilder` that allows for additional settings.
#[derive(Clone)]
pub struct BoringTlsConnector {
    /// The inner `SslConnectorBuilder`.
    builder: Arc<Builder>,
    /// The cached `HttpsConnector` sessions.
    session: Arc<OnceCell<Session>>,
}

impl BoringTlsConnector {
    /// Create a new `BoringTlsConnector` with the given function.
    pub fn new<F>(builder: F) -> BoringTlsConnector
    where
        F: Fn() -> Result<SslConnectorBuilder, ErrorStack> + Send + Sync + 'static,
    {
        Self {
            builder: Arc::new(builder),
            session: Arc::new(OnceCell::new()),
        }
    }

    /// Create a new `HttpsConnector` with the settings from the `ImpersonateContext`.
    #[inline]
    pub(crate) async fn create_connector(
        &self,
        context: &ImpersonateContext,
        http: HttpConnector,
    ) -> Result<HttpsConnector<HttpConnector>, ErrorStack> {
        // Create the `SslConnectorBuilder` and configure it.
        let builder = (self.builder)()?
            .configure_alpn_protos(context.h2)?
            .configure_cert_verification(context.certs_verification)?;

        // Check if the PSK extension should be enabled.
        let psk_extension = matches!(
            context.impersonate,
            Impersonate::Chrome116
                | Impersonate::Chrome117
                | Impersonate::Chrome120
                | Impersonate::Chrome123
                | Impersonate::Chrome124
                | Impersonate::Chrome126
                | Impersonate::Chrome127
                | Impersonate::Cronet
                | Impersonate::Edge122
                | Impersonate::Edge127
        );

        // Create the `HttpsConnector` with the given settings.
        let mut http = if psk_extension || context.pre_shared_key {
            // Initialize the session cache.
            let session = self
                .session
                .get_or_init(|| async {
                    Session::new(Mutex::new(SessionCache::with_capacity(
                        DEFAULT_SESSION_CACHE_CAPACITY,
                    )))
                })
                .await
                .clone();

            HttpsConnector::with_connector_and_settings(
                http,
                builder,
                HttpsLayerSettings::builder()
                    .session_cache_capacity(DEFAULT_SESSION_CACHE_CAPACITY)
                    .session_cache(session)
                    .build(),
            )?
        } else {
            HttpsConnector::with_connector(http, builder)?
        };

        // Set the callback to add application settings.
        let context = context.clone();
        http.set_callback(move |conf, _| {
            configure_ssl_context(conf, &context);
            Ok(())
        });
        Ok(http)
    }

    /// Create a new `SslConnector` with the settings from the `ImpersonateContext`.
    #[cfg(feature = "socks")]
    #[inline]
    pub(crate) async fn create_ssl(
        &self,
        context: &ImpersonateContext,
        http: HttpConnector,
        uri: &http::uri::Uri,
        host: &str,
    ) -> Result<Ssl, ErrorStack> {
        let connector = self.create_connector(context, http).await?;
        connector.setup_ssl(uri, host)
    }
}

/// Add application settings to the given `ConnectConfiguration`.
fn configure_ssl_context(conf: &mut ConnectConfiguration, ctx: &ImpersonateContext) {
    if matches!(
        ctx.impersonate.profile(),
        ClientProfile::Chrome | ClientProfile::Edge
    ) {
        conf.configure_permute_extensions(ctx.permute_extensions)
            .configure_enable_ech_grease(ctx.enable_ech_grease)
            .configure_add_application_settings(ctx.h2);
    }
}

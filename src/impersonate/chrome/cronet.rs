use super::CIPHER_LIST;
use crate::impersonate::extension::{ChromeExtension, Extension, SslExtension};
use crate::impersonate::profile::{Http2Settings, ImpersonateSettings};
use crate::impersonate::BoringTlsConnector;
use http::{
    header::{ACCEPT, ACCEPT_ENCODING, ACCEPT_LANGUAGE, UPGRADE_INSECURE_REQUESTS, USER_AGENT},
    HeaderMap, HeaderValue,
};

pub(crate) fn get_settings(headers: HeaderMap) -> ImpersonateSettings {
    ImpersonateSettings {
        tls_connector: BoringTlsConnector::new(|| {
            ChromeExtension::builder()?
                .configure_cipher_list(&CIPHER_LIST)?
                .configure_chrome_new_curves()
        }),
        http2: Http2Settings {
            initial_stream_window_size: Some(6291456),
            initial_connection_window_size: Some(15728640),
            max_concurrent_streams: None,
            max_header_list_size: Some(262144),
            header_table_size: Some(65536),
            enable_push: Some(false),
        },
        headers: create_headers(headers),
        gzip: true,
        brotli: true,
    }
}

fn create_headers(mut headers: HeaderMap) -> HeaderMap {
    headers.insert(
        ACCEPT_ENCODING,
        HeaderValue::from_static("gzip, deflate, br, zstd"),
    );
    headers
}

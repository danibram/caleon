use super::{ProviderError, ProviderErrorKind};
use encoding_rs::{Encoding, UTF_8};
use reqwest::{header, Client, RequestBuilder, Response, StatusCode};
use serde::de::DeserializeOwned;
use std::{
    collections::BTreeMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

type ResponseCache = Mutex<BTreeMap<String, (Instant, String)>>;
static CACHE: OnceLock<ResponseCache> = OnceLock::new();
const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

pub fn client() -> anyhow::Result<Client> {
    Ok(Client::builder()
        .timeout(Duration::from_secs(15))
        .connect_timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("caleon/", env!("CARGO_PKG_VERSION")))
        .build()?)
}

pub fn error(kind: ProviderErrorKind, message: &str) -> ProviderError {
    ProviderError {
        kind,
        message: message.into(),
    }
}

pub async fn json<T: DeserializeOwned>(request: RequestBuilder) -> Result<T, ProviderError> {
    let cache_key = request
        .try_clone()
        .and_then(|builder| builder.build().ok())
        .filter(|request| {
            !request.headers().contains_key("api_key")
                && !request.headers().contains_key(header::AUTHORIZATION)
        })
        .map(|request| request.url().to_string());
    if let Some(key) = &cache_key {
        let cache = CACHE
            .get_or_init(|| Mutex::new(BTreeMap::new()))
            .lock()
            .unwrap();
        if let Some((_, text)) = cache
            .get(key)
            .filter(|(when, _)| when.elapsed() < Duration::from_secs(60))
        {
            return serde_json::from_str(text).map_err(|_| {
                error(
                    ProviderErrorKind::InvalidResponse,
                    "No se pudo leer la respuesta meteorológica guardada",
                )
            });
        }
    }
    let response = request.send().await.map_err(|_| {
        error(
            ProviderErrorKind::Unavailable,
            "El proveedor meteorológico no responde",
        )
    })?;
    let kind = match response.status() {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Some(ProviderErrorKind::Authentication),
        StatusCode::TOO_MANY_REQUESTS => Some(ProviderErrorKind::RateLimited),
        status if status.is_server_error() => Some(ProviderErrorKind::Unavailable),
        status if !status.is_success() => Some(ProviderErrorKind::InvalidResponse),
        _ => None,
    };
    if let Some(kind) = kind {
        return Err(error(
            kind,
            if kind == ProviderErrorKind::Authentication {
                "El proveedor ha rechazado la clave"
            } else {
                "El proveedor no ha podido atender la consulta"
            },
        ));
    }
    let text = limited_text(response).await?;
    let result = serde_json::from_str(&text).map_err(|_| {
        error(
            ProviderErrorKind::InvalidResponse,
            "El proveedor devolvió datos que no se pueden interpretar",
        )
    });
    if result.is_ok() && text.len() <= 4 * 1024 * 1024 {
        if let Some(key) = cache_key {
            let mut cache = CACHE
                .get_or_init(|| Mutex::new(BTreeMap::new()))
                .lock()
                .unwrap();
            cache.retain(|_, (when, _)| when.elapsed() < Duration::from_secs(60));
            if cache.len() >= 16 {
                cache.clear();
            }
            cache.insert(key, (Instant::now(), text));
        }
    }
    result
}

async fn limited_text(mut response: Response) -> Result<String, ProviderError> {
    let too_large = || {
        error(
            ProviderErrorKind::InvalidResponse,
            "La respuesta meteorológica es demasiado grande",
        )
    };
    if response
        .content_length()
        .is_some_and(|size| size > MAX_RESPONSE_BYTES as u64)
    {
        return Err(too_large());
    }
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<mime::Mime>().ok());
    let encoding = content_type
        .as_ref()
        .and_then(|mime| mime.get_param("charset"))
        .and_then(|charset| Encoding::for_label(charset.as_str().as_bytes()))
        .unwrap_or(UTF_8);
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| {
        error(
            ProviderErrorKind::Unavailable,
            "No se pudo descargar la respuesta meteorológica",
        )
    })? {
        if chunk.len() > MAX_RESPONSE_BYTES - bytes.len() {
            return Err(too_large());
        }
        bytes.extend_from_slice(&chunk);
    }
    // Match reqwest text decoding, including BOM detection and replacement of malformed bytes.
    let (text, _, _) = encoding.decode(&bytes);
    Ok(text.into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, net::TcpListener, thread};

    fn response_server(
        body: impl FnOnce(std::net::TcpStream) + Send + 'static,
    ) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            std::io::Read::read(&mut socket, &mut request).unwrap();
            body(socket);
        });
        (url, handle)
    }

    #[tokio::test]
    async fn chunked_response_without_content_length_cannot_exceed_the_limit() {
        let (url, server) = response_server(|mut socket| {
            socket.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n").unwrap();
            let chunk = vec![b' '; 64 * 1024];
            for _ in 0..=(MAX_RESPONSE_BYTES / chunk.len()) {
                if write!(socket, "{:x}\r\n", chunk.len()).is_err()
                    || socket.write_all(&chunk).is_err()
                    || socket.write_all(b"\r\n").is_err()
                {
                    return;
                }
            }
            let _ = socket.write_all(b"0\r\n\r\n");
        });
        let response = client().unwrap().get(url).send().await.unwrap();
        assert_eq!(response.content_length(), None);
        let err = limited_text(response).await.unwrap_err();
        assert_eq!(err.kind, ProviderErrorKind::InvalidResponse);
        assert!(err.message.contains("demasiado grande"));
        server.join().unwrap();
    }

    #[tokio::test]
    async fn bounded_reader_preserves_aemet_charset_and_bom_decoding() {
        for (body, expected) in [
            (b"{\"name\":\"Pe\xf1alara \xa4\"}".as_slice(), "Peñalara €"),
            (
                b"\xef\xbb\xbf{\"name\":\"Pe\xc3\xb1alara\"}".as_slice(),
                "Peñalara",
            ),
        ] {
            let (url, server) = response_server(move |mut socket| {
                socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=\"iso-8859-15\"\r\nConnection: close\r\n\r\n").unwrap();
                socket.write_all(body).unwrap();
            });
            let data: serde_json::Value = json(client().unwrap().get(url)).await.unwrap();
            assert_eq!(data["name"], expected);
            server.join().unwrap();
        }
    }
}

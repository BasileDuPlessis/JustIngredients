//! The HTML pages of the doors (the sign-in, the photo upload): server rendered, no script, never
//! framed nor cached, no referrer (the addresses carry codes, states and links' secrets).

use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::engine::Lang;

/// The headers every sign-in page and redirect carries.
pub fn protective_headers() -> [(header::HeaderName, HeaderValue); 3] {
    [
        (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
        (
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ),
        (header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY")),
    ]
}

/// A page in French titled `title` around `body` (already escaped HTML), without forms.
pub fn html(status: StatusCode, title: &str, body: &str) -> Response {
    html_in(Lang::Fr, status, title, body)
}

/// [`html`], in `lang`.
pub fn html_in(lang: Lang, status: StatusCode, title: &str, body: &str) -> Response {
    html_page(lang, status, title, body, PageOptions::default())
}

/// What a page may do beyond showing text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PageOptions {
    /// It holds a form posting to the same origin.
    pub form: bool,
    /// It reloads itself after so many seconds (a reading under way).
    pub refresh_secs: Option<u32>,
}

/// A page in `lang` titled `title` around `body` (already escaped HTML), with `options`.
pub fn html_page(
    lang: Lang,
    status: StatusCode,
    title: &str,
    body: &str,
    options: PageOptions,
) -> Response {
    let refresh = options
        .refresh_secs
        .map(|secs| format!("<meta http-equiv=\"refresh\" content=\"{secs}\">"))
        .unwrap_or_default();
    let document = format!(
        "<!doctype html><html lang=\"{code}\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">{refresh}\
         <title>{title}</title><style>body{{font-family:system-ui,sans-serif;max-width:36rem;\
         margin:2rem auto;padding:0 1rem;line-height:1.5}}code{{word-break:break-all}}\
         button,input{{font-size:1.1rem;margin:.5rem 0}}</style>\
         </head><body><h1>{title}</h1>{body}</body></html>",
        title = escape(title),
        code = if lang.is_french() { "fr" } else { "en" },
    );
    let policy = if options.form {
        "default-src 'none'; style-src 'unsafe-inline'; form-action 'self'; \
         frame-ancestors 'none'; base-uri 'none'"
    } else {
        "default-src 'none'; style-src 'unsafe-inline'; form-action 'none'; \
         frame-ancestors 'none'; base-uri 'none'"
    };
    (
        status,
        protective_headers(),
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/html; charset=utf-8"),
            ),
            (
                header::CONTENT_SECURITY_POLICY,
                HeaderValue::from_static(policy),
            ),
        ],
        document,
    )
        .into_response()
}

/// The language of a page: French when the browser asks for it first, else English.
pub fn lang_of(headers: &HeaderMap) -> Lang {
    let first = headers
        .get(header::ACCEPT_LANGUAGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .and_then(|tag| tag.split(';').next())
        .map(|value| value.trim().to_ascii_lowercase());
    Lang::from_code(first.as_deref())
}

/// French or English text.
pub fn text(lang: Lang, fr: &'static str, en: &'static str) -> &'static str {
    if lang.is_french() { fr } else { en }
}

/// A 303 to `location`, with the protective headers.
pub fn see_other(location: &str) -> Response {
    match HeaderValue::from_str(location) {
        Ok(location) => (
            StatusCode::SEE_OTHER,
            protective_headers(),
            [(header::LOCATION, location)],
        )
            .into_response(),
        Err(_) => html(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Erreur",
            "<p>Adresse de redirection invalide.</p>",
        ),
    }
}

/// Text made safe inside HTML, in elements and in quoted attributes.
pub fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            c => escaped.push(c),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_and_quotes_are_escaped() {
        assert_eq!(
            escape("<script>\"a\" & 'b'</script>"),
            "&lt;script&gt;&quot;a&quot; &amp; &#39;b&#39;&lt;/script&gt;"
        );
    }
}

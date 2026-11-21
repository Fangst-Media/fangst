//! Request parameters from the query string and the body, as SABnzbd reads them: clients put
//! `apikey`, `mode` and the rest in either place, and upload NZBs as multipart form data.

use fangst_providers::HttpRequest;

/// One uploaded file from a multipart body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Upload {
    pub field: String,
    pub filename: Option<String>,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Params {
    fields: Vec<(String, String)>,
    pub uploads: Vec<Upload>,
}

impl Params {
    /// The first value for `name`; query parameters come before body fields.
    pub(crate) fn get(&self, name: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// Like [`Params::get`], but an empty value counts as absent, as SABnzbd treats `cat=`.
    pub(crate) fn non_empty(&self, name: &str) -> Option<&str> {
        self.get(name).filter(|v| !v.trim().is_empty())
    }
}

/// Reads the query and, for form bodies, the body. A body that claims to be multipart but
/// doesn't parse contributes nothing; the mode then reports what is missing.
pub(crate) fn parse(request: &HttpRequest) -> Params {
    let mut params = Params {
        fields: request.query.clone(),
        uploads: Vec::new(),
    };
    let content_type = request.header("content-type").unwrap_or_default();
    let essence = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    match essence.as_str() {
        "application/x-www-form-urlencoded" => params
            .fields
            .extend(form_urlencoded::parse(&request.body).into_owned()),
        "multipart/form-data" => {
            if let Some(boundary) = boundary(content_type) {
                for part in multipart(&request.body, boundary.as_bytes()) {
                    match part.filename {
                        Some(_) => params.uploads.push(Upload {
                            field: part.name,
                            filename: part.filename,
                            bytes: part.body.to_vec(),
                        }),
                        None => params
                            .fields
                            .push((part.name, String::from_utf8_lossy(part.body).into_owned())),
                    }
                }
            }
        }
        _ => {}
    }
    params
}

fn boundary(content_type: &str) -> Option<String> {
    content_type.split(';').skip(1).find_map(|param| {
        let (key, value) = param.split_once('=')?;
        key.trim()
            .eq_ignore_ascii_case("boundary")
            .then(|| value.trim().trim_matches('"').to_owned())
            .filter(|b| !b.is_empty())
    })
}

struct Part<'a> {
    name: String,
    filename: Option<String>,
    body: &'a [u8],
}

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    haystack
        .get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|i| i + from)
}

/// Parts of a `multipart/form-data` body (RFC 7578). Stops at the first malformed part.
fn multipart<'a>(body: &'a [u8], boundary: &[u8]) -> Vec<Part<'a>> {
    let mut delimiter = b"--".to_vec();
    delimiter.extend_from_slice(boundary);
    let mut parts = Vec::new();
    let Some(first) = find(body, &delimiter, 0) else {
        return parts;
    };
    let mut cursor = first + delimiter.len();
    // Every later delimiter starts on its own line.
    let mut next_delimiter = b"\r\n".to_vec();
    next_delimiter.extend_from_slice(&delimiter);
    loop {
        if body.get(cursor..cursor + 2) == Some(b"--") {
            break;
        }
        let Some(headers_start) = find(body, b"\r\n", cursor).map(|i| i + 2) else {
            break;
        };
        let Some(headers_end) = find(body, b"\r\n\r\n", headers_start) else {
            break;
        };
        let Some(end) = find(body, &next_delimiter, headers_end + 4) else {
            break;
        };
        let headers = String::from_utf8_lossy(&body[headers_start..headers_end]);
        if let Some((name, filename)) = disposition(&headers) {
            parts.push(Part {
                name,
                filename,
                body: &body[headers_end + 4..end],
            });
        }
        cursor = end + next_delimiter.len();
    }
    parts
}

/// `name` and `filename` from a part's `Content-Disposition: form-data` header.
fn disposition(headers: &str) -> Option<(String, Option<String>)> {
    let line = headers.split("\r\n").find(|line| {
        line.split_once(':')
            .is_some_and(|(k, _)| k.trim().eq_ignore_ascii_case("content-disposition"))
    })?;
    let (_, value) = line.split_once(':')?;
    let mut name = None;
    let mut filename = None;
    for param in value.split(';').skip(1) {
        let Some((key, value)) = param.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"').to_owned();
        match key.trim().to_ascii_lowercase().as_str() {
            "name" => name = Some(value),
            "filename" => filename = Some(value),
            _ => {}
        }
    }
    Some((name?, filename))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(content_type: &str, body: &[u8], query: &[(&str, &str)]) -> HttpRequest {
        HttpRequest {
            method: "POST".to_owned(),
            path: "/".to_owned(),
            query: query
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
            headers: vec![("content-type".to_owned(), content_type.to_owned())],
            body: body.to_vec(),
        }
    }

    #[test]
    fn multipart_fields_and_files() {
        let body = b"--XyZ\r\nContent-Disposition: form-data; name=\"apikey\"\r\n\r\nk1\r\n--XyZ\r\nContent-Disposition: form-data; name=\"name\"; filename=\"Show.nzb\"\r\nContent-Type: application/x-nzb\r\n\r\n<nzb>\r\n--x</nzb>\r\n--XyZ--\r\n";
        let params = parse(&request(
            "multipart/form-data; boundary=\"XyZ\"",
            body,
            &[("mode", "addfile")],
        ));
        assert_eq!(params.get("mode"), Some("addfile"));
        assert_eq!(params.get("apikey"), Some("k1"));
        assert_eq!(
            params.uploads,
            [Upload {
                field: "name".to_owned(),
                filename: Some("Show.nzb".to_owned()),
                bytes: b"<nzb>\r\n--x</nzb>".to_vec(),
            }]
        );
    }

    #[test]
    fn query_wins_over_the_form_body() {
        let params = parse(&request(
            "application/x-www-form-urlencoded",
            b"mode=queue&apikey=body&cat=",
            &[("apikey", "query")],
        ));
        assert_eq!(params.get("apikey"), Some("query"));
        assert_eq!(params.get("mode"), Some("queue"));
        assert_eq!(params.get("cat"), Some(""));
        assert_eq!(params.non_empty("cat"), None);
    }

    #[test]
    fn broken_multipart_gives_nothing_rather_than_failing() {
        for body in [
            &b""[..],
            b"--XyZ",
            b"--XyZ\r\nContent-Disposition: form-data; name=\"a\"\r\n\r\nno end",
            b"garbage",
        ] {
            let params = parse(&request("multipart/form-data; boundary=XyZ", body, &[]));
            assert_eq!(params, Params::default());
        }
    }
}

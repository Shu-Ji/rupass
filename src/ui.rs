use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

use anyhow::{Context, Result, anyhow};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::storage::AppPaths;
use crate::ui_api::{
    TeamSummary, create_team, delete_secret, delete_team, get_secret, list_keys, read_teams,
    set_remote, set_secret, sync_all, sync_team,
};
use crate::ui_assets::INDEX_HTML;
use crate::ui_script::SCRIPT_JS;
use crate::ui_style::STYLE_CSS;

#[derive(Deserialize)]
struct TeamCreateRequest {
    team: String,
    display_name: Option<String>,
    password: String,
    password_confirm: String,
}

#[derive(Deserialize)]
struct PasswordRequest {
    password: String,
}

#[derive(Deserialize)]
struct SecretGetRequest {
    key: String,
}

#[derive(Deserialize)]
struct SecretSetRequest {
    key: String,
    value: String,
    password: String,
}

#[derive(Deserialize)]
struct SecretDeleteRequest {
    key: String,
    password: String,
}

#[derive(Deserialize)]
struct RemoteSetRequest {
    url: String,
    password: String,
}

#[derive(Deserialize)]
struct SyncAllRequest {
    passwords: HashMap<String, String>,
}

#[derive(Serialize)]
struct TeamsResponse {
    teams: Vec<TeamSummary>,
}

#[derive(Serialize)]
struct KeysResponse {
    keys: Vec<String>,
}

#[derive(Serialize)]
struct SecretValueResponse {
    value: String,
}

#[derive(Serialize)]
struct ApiErrorResponse {
    error: String,
}

struct HttpRequest {
    method: String,
    path: String,
    body: Vec<u8>,
}

struct HttpResponse {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

pub(crate) fn run(paths: AppPaths) -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").context("failed to bind local ui server")?;
    let addr = listener
        .local_addr()
        .context("failed to read local ui address")?;
    println!("rupass ui listening on http://{addr}");

    for stream in listener.incoming() {
        let paths = paths.clone();
        match stream {
            Ok(stream) => {
                thread::spawn(move || {
                    if let Err(err) = handle_connection(stream, &paths) {
                        eprintln!("ui error: {err}");
                    }
                });
            }
            Err(err) => eprintln!("ui accept error: {err}"),
        }
    }
    Ok(())
}

fn handle_connection(mut stream: TcpStream, paths: &AppPaths) -> Result<()> {
    let request = read_request(&mut stream)?;
    let response = match route_request(paths, &request) {
        Ok(response) => response,
        Err(err) => json_error(400, err.to_string()),
    };
    write_response(&mut stream, response)?;
    Ok(())
}

fn route_request(paths: &AppPaths, request: &HttpRequest) -> Result<HttpResponse> {
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/") => return Ok(text_response(200, "text/html; charset=utf-8", INDEX_HTML)),
        ("GET", "/ui.css") => {
            return Ok(text_response(200, "text/css; charset=utf-8", STYLE_CSS));
        }
        ("GET", "/ui.js") => {
            return Ok(text_response(
                200,
                "application/javascript; charset=utf-8",
                SCRIPT_JS,
            ));
        }
        ("GET", "/favicon.ico") => {
            return Ok(HttpResponse {
                status: 204,
                content_type: "text/plain; charset=utf-8",
                body: Vec::new(),
            });
        }
        ("GET", "/api/teams") => {
            return json_response(&TeamsResponse {
                teams: read_teams(paths)?,
            });
        }
        ("POST", "/api/teams") => {
            let payload: TeamCreateRequest = parse_json(request)?;
            create_team(
                paths,
                &payload.team,
                payload.display_name.as_deref(),
                &payload.password,
                &payload.password_confirm,
            )?;
            return json_response(&TeamsResponse {
                teams: read_teams(paths)?,
            });
        }
        ("POST", "/api/sync-all") => {
            let payload: SyncAllRequest = parse_json(request)?;
            sync_all(paths, &payload.passwords)?;
            return json_response(&serde_json::json!({ "ok": true }));
        }
        _ => {}
    }

    let parts: Vec<&str> = request.path.trim_matches('/').split('/').collect();
    if parts.len() >= 3 && parts[0] == "api" && parts[1] == "teams" {
        let team = parts[2];
        match (
            request.method.as_str(),
            parts.get(3).copied(),
            parts.get(4).copied(),
        ) {
            ("POST", Some("delete"), None) => {
                let payload: PasswordRequest = parse_json(request)?;
                delete_team(paths, team, &payload.password)?;
                return json_response(&serde_json::json!({ "ok": true }));
            }
            ("POST", Some("remote"), None) => {
                let payload: RemoteSetRequest = parse_json(request)?;
                set_remote(paths, team, &payload.url, &payload.password)?;
                return json_response(&serde_json::json!({ "ok": true }));
            }
            ("POST", Some("sync"), None) => {
                let payload: PasswordRequest = parse_json(request)?;
                sync_team(paths, team, &payload.password)?;
                return json_response(&serde_json::json!({ "ok": true }));
            }
            ("POST", Some("secrets"), Some("list")) => {
                let payload: PasswordRequest = parse_json(request)?;
                return json_response(&KeysResponse {
                    keys: list_keys(paths, team, &payload.password)?,
                });
            }
            ("POST", Some("secrets"), Some("get")) => {
                let payload: SecretGetRequest = parse_json(request)?;
                return json_response(&SecretValueResponse {
                    value: get_secret(paths, team, &payload.key)?,
                });
            }
            ("POST", Some("secrets"), Some("set")) => {
                let payload: SecretSetRequest = parse_json(request)?;
                set_secret(paths, team, &payload.key, &payload.value, &payload.password)?;
                return json_response(&serde_json::json!({ "ok": true }));
            }
            ("POST", Some("secrets"), Some("delete")) => {
                let payload: SecretDeleteRequest = parse_json(request)?;
                delete_secret(paths, team, &payload.key, &payload.password)?;
                return json_response(&serde_json::json!({ "ok": true }));
            }
            _ => {}
        }
    }

    Ok(json_error(404, "not found"))
}

fn parse_json<T: DeserializeOwned>(request: &HttpRequest) -> Result<T> {
    serde_json::from_slice(&request.body).context("invalid json payload")
}

fn read_request(stream: &mut TcpStream) -> Result<HttpRequest> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];
    let mut header_end = None;
    let mut body_len = 0;

    loop {
        let read = stream
            .read(&mut chunk)
            .context("failed to read http request")?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if header_end.is_none()
            && let Some(end) = find_bytes(&buffer, b"\r\n\r\n")
        {
            header_end = Some(end + 4);
            body_len = content_length(&buffer[..end + 4])?;
        }
        if let Some(end) = header_end
            && buffer.len() >= end + body_len
        {
            break;
        }
    }

    let header_end = header_end.ok_or_else(|| anyhow!("invalid http request"))?;
    let header = String::from_utf8_lossy(&buffer[..header_end - 4]);
    let mut lines = header.lines();
    let request_line = lines
        .next()
        .ok_or_else(|| anyhow!("missing request line"))?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().ok_or_else(|| anyhow!("missing http method"))?;
    let raw_path = parts.next().ok_or_else(|| anyhow!("missing http path"))?;

    Ok(HttpRequest {
        method: method.to_string(),
        path: raw_path.split('?').next().unwrap_or(raw_path).to_string(),
        body: buffer[header_end..].to_vec(),
    })
}

fn content_length(header: &[u8]) -> Result<usize> {
    let text = String::from_utf8_lossy(header);
    for line in text.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("content-length") {
            return value.trim().parse().context("invalid content-length");
        }
    }
    Ok(0)
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn write_response(stream: &mut TcpStream, response: HttpResponse) -> Result<()> {
    let reason = match response.status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "Internal Server Error",
    };
    write!(
        stream,
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        reason,
        response.content_type,
        response.body.len()
    )
    .context("failed to write http header")?;
    stream
        .write_all(&response.body)
        .context("failed to write http body")?;
    Ok(())
}

fn text_response(status: u16, content_type: &'static str, body: &str) -> HttpResponse {
    HttpResponse {
        status,
        content_type,
        body: body.as_bytes().to_vec(),
    }
}

fn json_response<T: Serialize>(value: &T) -> Result<HttpResponse> {
    Ok(HttpResponse {
        status: 200,
        content_type: "application/json; charset=utf-8",
        body: serde_json::to_vec(value).context("failed to serialize json response")?,
    })
}

fn json_error(status: u16, message: impl Into<String>) -> HttpResponse {
    let body = serde_json::to_vec(&ApiErrorResponse {
        error: message.into(),
    })
    .unwrap_or_else(|_| b"{\"error\":\"internal error\"}".to_vec());
    HttpResponse {
        status,
        content_type: "application/json; charset=utf-8",
        body,
    }
}

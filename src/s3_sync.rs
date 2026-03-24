use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    fs,
    net::IpAddr,
    path::Path,
    time::UNIX_EPOCH,
};

use anyhow::{Context, Result, anyhow, bail};
use hyper::{Body, Method, Request, StatusCode, body::to_bytes};
use hyper_rustls::HttpsConnectorBuilder;
use rand::{Rng, distributions::Alphanumeric};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use url::Url;

use crate::storage::{TeamConfig, TeamMetadata, TeamS3Config};

const VIRTUAL_HOST_SUFFIXES: &[&str] = &["aliyuncs.com", "myqcloud.com", "volces.com"];
const CHERRY_X_AMZ_USER_AGENT: &str = "aws-sdk-js/3.998.0";
const CHERRY_USER_AGENT: &str =
    "aws-sdk-js/3.998.0 ua/2.1 os/darwin#25.3.0 lang/js md/nodejs#24.14.0 api/s3#3.998.0 m/N,E,e";
const TEAM_METADATA_FILE: &str = "rupass-team.json";
const LEGACY_TEAM_METADATA_FILE: &str = ".rupass-team.json";
const REMOTE_MANIFEST_FILE: &str = "rupass-manifest.json";
const LEGACY_REMOTE_MANIFEST_FILE: &str = ".rupass-manifest.json";
const LOCAL_STATE_FILE: &str = "rupass-s3-state.json";
const LEGACY_LOCAL_STATE_FILE: &str = ".rupass-s3-state.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
struct Manifest {
    files: BTreeMap<String, ManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ManifestEntry {
    sha256: String,
    modified_unix: i64,
    size: u64,
}

#[derive(Debug)]
struct RawResponse {
    status: StatusCode,
    body: Vec<u8>,
}

pub(crate) fn sync_team_store(repo_dir: &Path, config: &TeamConfig) -> Result<()> {
    let Some(s3) = config.s3.clone() else {
        return Ok(());
    };

    fs::create_dir_all(repo_dir)
        .with_context(|| format!("failed to create repo dir {}", repo_dir.display()))?;
    cleanup_local_internal_files(repo_dir)?;
    ensure_local_metadata(repo_dir, config)?;

    let client = S3Client::new(s3, config.team_name.clone())?;
    let local_manifest = scan_local_manifest(repo_dir)?;
    let remote_manifest = normalize_manifest(client.get_manifest()?);
    let state = read_local_state(repo_dir, &config.team_name)?;

    if state.files.is_empty() {
        initial_sync(&client, repo_dir, &local_manifest, &remote_manifest)?;
    } else {
        incremental_sync(&client, repo_dir, &local_manifest, &remote_manifest, &state)?;
    }

    let final_manifest = scan_local_manifest(repo_dir)?;
    client.put_manifest(&final_manifest)?;
    write_local_state(repo_dir, &config.team_name, &final_manifest)?;
    cleanup_local_internal_files(repo_dir)?;
    Ok(())
}

fn initial_sync(
    client: &S3Client,
    repo_dir: &Path,
    local_manifest: &Manifest,
    remote_manifest: &Manifest,
) -> Result<()> {
    if local_manifest == remote_manifest {
        return Ok(());
    }

    let conflict_paths = detect_conflicts(local_manifest, remote_manifest, &Manifest::default());
    if !conflict_paths.is_empty() {
        bail!(
            "initial S3 merge conflict on team {}\nconflicts: {}\n请先确认保留哪一端，再重新同步",
            client.team_name,
            conflict_paths.join(", ")
        );
    }

    let changed_local = diff_paths(local_manifest, &Manifest::default());
    let changed_remote = diff_paths(remote_manifest, &Manifest::default());

    for relative_path in changed_remote.difference(&changed_local) {
        match remote_manifest.files.get(relative_path.as_str()) {
            Some(_) => client.download_file(repo_dir, relative_path)?,
            None => delete_local_file(repo_dir, relative_path)?,
        }
    }

    for relative_path in changed_local.difference(&changed_remote) {
        match local_manifest.files.get(relative_path.as_str()) {
            Some(_) => client.upload_file(repo_dir, relative_path)?,
            None => client.delete_file(relative_path)?,
        }
    }

    for relative_path in changed_local.intersection(&changed_remote) {
        if local_manifest.files.get(relative_path.as_str()).is_some()
            && remote_manifest.files.get(relative_path.as_str()).is_none()
        {
            client.upload_file(repo_dir, relative_path)?;
        } else if local_manifest.files.get(relative_path.as_str()).is_none()
            && remote_manifest.files.get(relative_path.as_str()).is_some()
        {
            client.download_file(repo_dir, relative_path)?;
        }
    }

    Ok(())
}

fn incremental_sync(
    client: &S3Client,
    repo_dir: &Path,
    local_manifest: &Manifest,
    remote_manifest: &Manifest,
    state: &Manifest,
) -> Result<()> {
    let conflict_paths = detect_conflicts(local_manifest, remote_manifest, state);
    if !conflict_paths.is_empty() {
        bail!(
            "S3 sync conflict on team {}\nconflicts: {}\n请在一端完成修改后再次同步",
            client.team_name,
            conflict_paths.join(", ")
        );
    }

    let changed_local = diff_paths(local_manifest, state);
    let changed_remote = diff_paths(remote_manifest, state);

    for relative_path in changed_remote.difference(&changed_local) {
        match remote_manifest.files.get(relative_path.as_str()) {
            Some(_) => client.download_file(repo_dir, relative_path)?,
            None => delete_local_file(repo_dir, relative_path)?,
        }
    }

    for relative_path in changed_local.difference(&changed_remote) {
        match local_manifest.files.get(relative_path.as_str()) {
            Some(_) => client.upload_file(repo_dir, relative_path)?,
            None => client.delete_file(relative_path)?,
        }
    }

    Ok(())
}

fn diff_paths(current: &Manifest, base: &Manifest) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    for path in current
        .files
        .keys()
        .chain(base.files.keys())
        .map(String::as_str)
        .collect::<BTreeSet<_>>()
    {
        if !entries_equal(current.files.get(path), base.files.get(path)) {
            paths.insert(path.to_string());
        }
    }
    paths
}

fn detect_conflicts(current_local: &Manifest, current_remote: &Manifest, base: &Manifest) -> Vec<String> {
    let changed_local = diff_paths(current_local, base);
    let changed_remote = diff_paths(current_remote, base);
    changed_local
        .intersection(&changed_remote)
        .filter_map(|path| {
            if entries_equal(
                current_local.files.get(path.as_str()),
                current_remote.files.get(path.as_str()),
            ) {
                None
            } else {
                Some(path.to_string())
            }
        })
        .collect()
}

fn entries_equal(left: Option<&ManifestEntry>, right: Option<&ManifestEntry>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => left.sha256 == right.sha256 && left.size == right.size,
        _ => false,
    }
}

fn ensure_local_metadata(repo_dir: &Path, config: &TeamConfig) -> Result<()> {
    let path = repo_dir.join(TEAM_METADATA_FILE);
    let legacy_path = repo_dir.join(LEGACY_TEAM_METADATA_FILE);
    let expected = TeamMetadata::from(config);
    if path.exists() {
        let actual: TeamMetadata = read_json(&path)?;
        if actual != expected {
            bail!(
                "S3 team metadata does not match local config\nteam: {}\npath: {}",
                config.team_name,
                path.display()
            );
        }
        return Ok(());
    }

    if legacy_path.exists() {
        let actual: TeamMetadata = read_json(&legacy_path)?;
        if actual != expected {
            bail!(
                "S3 team metadata does not match local config\nteam: {}\npath: {}",
                config.team_name,
                legacy_path.display()
            );
        }
        fs::rename(&legacy_path, &path).with_context(|| {
            format!(
                "failed to migrate team metadata from {} to {}",
                legacy_path.display(),
                path.display()
            )
        })?;
        return Ok(());
    }

    write_json(&path, &expected)
}

fn scan_local_manifest(repo_dir: &Path) -> Result<Manifest> {
    let mut files = BTreeMap::new();
    scan_dir_recursive(repo_dir, repo_dir, &mut files)?;
    Ok(Manifest { files })
}

fn scan_dir_recursive(
    root: &Path,
    dir: &Path,
    files: &mut BTreeMap<String, ManifestEntry>,
) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };

        if path.is_dir() {
            if name == ".git" || name.starts_with('.') {
                continue;
            }
            scan_dir_recursive(root, &path, files)?;
            continue;
        }

        if !should_sync_local_file(name) {
            continue;
        }

        let relative = path
            .strip_prefix(root)
            .with_context(|| format!("failed to build relative path for {}", path.display()))?;
        let relative = path_to_key(relative);
        files.insert(relative, manifest_entry_for_path(&path)?);
    }
    Ok(())
}

fn should_sync_local_file(name: &str) -> bool {
    if name == LOCAL_STATE_FILE
        || name == LEGACY_LOCAL_STATE_FILE
        || name == REMOTE_MANIFEST_FILE
        || name == LEGACY_REMOTE_MANIFEST_FILE
    {
        return false;
    }
    if name == TEAM_METADATA_FILE || name == LEGACY_TEAM_METADATA_FILE {
        return true;
    }
    !name.starts_with('.')
}

fn manifest_entry_for_path(path: &Path) -> Result<ManifestEntry> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let metadata = fs::metadata(path).with_context(|| format!("failed to stat {}", path.display()))?;
    let modified = metadata
        .modified()
        .with_context(|| format!("failed to read modified time {}", path.display()))?
        .duration_since(UNIX_EPOCH)
        .context("file modified time is before unix epoch")?
        .as_secs() as i64;

    Ok(ManifestEntry {
        sha256: sha256_hex(&bytes),
        modified_unix: modified,
        size: bytes.len() as u64,
    })
}

fn read_local_state(repo_dir: &Path, team_name: &str) -> Result<Manifest> {
    let path = external_local_state_path(repo_dir, team_name)?;
    if path.exists() {
        return read_json(&path);
    }
    let legacy_path = repo_dir.join(LOCAL_STATE_FILE);
    if legacy_path.exists() {
        let manifest: Manifest = read_json(&legacy_path)?;
        write_json(&path, &manifest)?;
        let _ = fs::remove_file(&legacy_path);
        return Ok(manifest);
    }
    let legacy_path = repo_dir.join(LEGACY_LOCAL_STATE_FILE);
    if legacy_path.exists() {
        let manifest: Manifest = read_json(&legacy_path)?;
        write_json(&path, &manifest)?;
        let _ = fs::remove_file(&legacy_path);
        return Ok(manifest);
    }
    Ok(Manifest::default())
}

fn write_local_state(repo_dir: &Path, team_name: &str, manifest: &Manifest) -> Result<()> {
    let path = external_local_state_path(repo_dir, team_name)?;
    write_json(&path, manifest)?;
    for legacy_path in [
        repo_dir.join(LOCAL_STATE_FILE),
        repo_dir.join(LEGACY_LOCAL_STATE_FILE),
    ] {
        if legacy_path.exists() {
            let _ = fs::remove_file(legacy_path);
        }
    }
    Ok(())
}

fn cleanup_local_internal_files(repo_dir: &Path) -> Result<()> {
    for file_name in [
        REMOTE_MANIFEST_FILE,
        LEGACY_REMOTE_MANIFEST_FILE,
        LOCAL_STATE_FILE,
        LEGACY_LOCAL_STATE_FILE,
    ] {
        let path = repo_dir.join(file_name);
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("failed to remove stale internal file {}", path.display()))?;
        }
    }
    Ok(())
}

fn delete_local_file(repo_dir: &Path, relative_path: &str) -> Result<()> {
    let path = repo_dir.join(relative_path);
    if path.exists() {
        fs::remove_file(&path).with_context(|| format!("failed to delete {}", path.display()))?;
        cleanup_empty_parents(repo_dir, path.parent());
    }
    Ok(())
}

fn cleanup_empty_parents(root: &Path, mut current: Option<&Path>) {
    while let Some(dir) = current {
        if dir == root {
            break;
        }
        let is_empty = fs::read_dir(dir)
            .ok()
            .and_then(|mut entries| entries.next().transpose().ok())
            .flatten()
            .is_none();
        if is_empty {
            let _ = fs::remove_dir(dir);
            current = dir.parent();
        } else {
            break;
        }
    }
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value).context("failed to serialize json")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, bytes).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("failed to parse {}", path.display()))
}

fn path_to_key(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}

fn external_local_state_path(repo_dir: &Path, team_name: &str) -> Result<std::path::PathBuf> {
    let Some(store_dir) = repo_dir.parent() else {
        bail!("invalid repo dir: {}", repo_dir.display());
    };
    let Some(base_dir) = store_dir.parent() else {
        bail!("invalid repo dir: {}", repo_dir.display());
    };
    Ok(base_dir
        .join("state")
        .join("s3")
        .join(format!("{team_name}.json")))
}

struct S3Client {
    config: TeamS3Config,
    team_name: String,
    http: hyper::Client<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>, Body>,
}

impl S3Client {
    fn new(config: TeamS3Config, team_name: String) -> Result<Self> {
        if config.endpoint.trim().is_empty()
            || config.region.trim().is_empty()
            || config.bucket.trim().is_empty()
            || config.access_key_id.trim().is_empty()
            || config.secret_access_key.trim().is_empty()
        {
            bail!("incomplete S3 config for team {team_name}");
        }
        let http = {
            let https = HttpsConnectorBuilder::new()
                .with_webpki_roots()
                .https_only()
                .enable_http1()
                .build();
            hyper::Client::builder().build(https)
        };
        Ok(Self {
            config,
            team_name,
            http,
        })
    }

    fn normalized_root(&self) -> String {
        self.config.root.trim_matches('/').to_string()
    }

    fn prefix(&self) -> String {
        self.normalized_root()
    }

    fn object_key(&self, relative_path: &str) -> String {
        let relative_path = relative_path.trim_start_matches('/');
        let prefix = self.prefix();
        if prefix.is_empty() {
            relative_path.to_string()
        } else {
            format!("{prefix}/{relative_path}")
        }
    }

    fn manifest_key(&self) -> String {
        self.object_key(REMOTE_MANIFEST_FILE)
    }

    fn legacy_manifest_key(&self) -> String {
        self.object_key(LEGACY_REMOTE_MANIFEST_FILE)
    }

    fn team_object_key(&self, relative_path: &str) -> String {
        self.object_key(relative_path)
    }

    fn get_manifest(&self) -> Result<Manifest> {
        let objects = self.list_objects()?;
        let manifest_key = if objects.iter().any(|key| key == &self.manifest_key()) {
            self.manifest_key()
        } else if objects.iter().any(|key| key == &self.legacy_manifest_key()) {
            self.legacy_manifest_key()
        } else {
            return Ok(Manifest::default());
        };

        match self.get_object(&manifest_key)? {
            Some(bytes) => match serde_json::from_slice(&bytes) {
                Ok(manifest) => Ok(manifest),
                Err(err) => {
                    if s3_trace_enabled() {
                        eprintln!(
                            "S3 TRACE invalid remote manifest ignored: key={} error={} body={}",
                            manifest_key,
                            err,
                            String::from_utf8_lossy(&bytes)
                        );
                    }
                    Ok(Manifest::default())
                }
            },
            None => Ok(Manifest::default()),
        }
    }

    fn put_manifest(&self, manifest: &Manifest) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(manifest).context("failed to serialize remote S3 manifest")?;
        self.put_object(&self.manifest_key(), bytes, "application/json")?;
        let _ = self.delete_object(&self.legacy_manifest_key());
        Ok(())
    }

    fn upload_file(&self, repo_dir: &Path, relative_path: &str) -> Result<()> {
        let path = repo_dir.join(relative_path);
        let bytes = fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
        let content_type = if relative_path.ends_with(".json") {
            "application/json"
        } else {
            "application/octet-stream"
        };
        self.put_object(&self.team_object_key(relative_path), bytes, content_type)
    }

    fn download_file(&self, repo_dir: &Path, relative_path: &str) -> Result<()> {
        let Some(bytes) = self.get_object(&self.team_object_key(relative_path))? else {
            bail!("remote S3 object missing during sync: {}", self.team_object_key(relative_path));
        };
        let path = repo_dir.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::write(&path, bytes).with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }

    fn delete_file(&self, relative_path: &str) -> Result<()> {
        self.delete_object(&self.team_object_key(relative_path))
    }

    fn list_objects(&self) -> Result<Vec<String>> {
        let prefix = self.prefix();
        let prefix = if prefix.is_empty() {
            String::new()
        } else {
            format!("{prefix}/")
        };
        let response = self.send_signed_request(
            Method::GET,
            None,
            &[
                ("list-type".to_string(), "2".to_string()),
                ("prefix".to_string(), prefix),
            ],
            Vec::new(),
            None,
        )
        .context("ListObjectsV2 transport failed")?;
        match response.status {
            StatusCode::OK => Ok(parse_list_object_keys(
                &String::from_utf8(response.body).context("list objects response is not valid UTF-8")?,
            )),
            _ => bail!(format_raw_http_error("ListObjectsV2", &response)),
        }
    }

    fn get_object(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let response = self
            .send_signed_request(Method::GET, Some(key), &[], Vec::new(), None)
            .with_context(|| format!("GetObject transport failed for key {key}"))?;
        match response.status {
            StatusCode::OK => Ok(Some(response.body)),
            StatusCode::NOT_FOUND => Ok(None),
            _ => bail!(format_raw_http_error("GetObject", &response)),
        }
    }

    fn put_object(&self, key: &str, body: Vec<u8>, content_type: &str) -> Result<()> {
        let response = self
            .send_signed_request(Method::PUT, Some(key), &[], body, Some(content_type))
            .with_context(|| format!("PutObject transport failed for key {key}"))?;
        match response.status {
            StatusCode::OK => Ok(()),
            _ => bail!(format_raw_http_error("PutObject", &response)),
        }
    }

    fn delete_object(&self, key: &str) -> Result<()> {
        let response = self
            .send_signed_request(Method::DELETE, Some(key), &[], Vec::new(), None)
            .with_context(|| format!("DeleteObject transport failed for key {key}"))?;
        match response.status {
            StatusCode::NO_CONTENT | StatusCode::OK | StatusCode::NOT_FOUND => Ok(()),
            _ => bail!(format_raw_http_error("DeleteObject", &response)),
        }
    }

    fn send_signed_request(
        &self,
        method: Method,
        key: Option<&str>,
        query: &[(String, String)],
        body: Vec<u8>,
        content_type: Option<&str>,
    ) -> Result<RawResponse> {
        tokio::runtime::Runtime::new()
            .context("failed to build tokio runtime for S3 request")?
            .block_on(self.send_signed_request_async(method, key, query, body, content_type))
    }

    async fn send_signed_request_async(
        &self,
        method: Method,
        key: Option<&str>,
        query: &[(String, String)],
        body: Vec<u8>,
        content_type: Option<&str>,
    ) -> Result<RawResponse> {
        let mut endpoint = Url::parse(&self.config.endpoint).context("invalid S3 endpoint URL")?;
        let force_path_style = if self.config.force_path_style {
            true
        } else {
            infer_force_path_style(&self.config.endpoint)
        };
        let (host_header, path) =
            canonical_target(&mut endpoint, &self.config.bucket, force_path_style, key)?;
        let query_string = canonical_query_string(query);
        let request_url = if query_string.is_empty() {
            format!("{}{}", endpoint.origin().ascii_serialization(), path)
        } else {
            format!("{}{}?{}", endpoint.origin().ascii_serialization(), path, query_string)
        };

        let now = OffsetDateTime::now_utc();
        let amz_date = format_amz_date(now);
        let short_date = format_short_date(now);
        let payload_hash = sha256_hex(&body);
        let invocation_id = random_invocation_id();

        let mut signed_headers = BTreeMap::new();
        signed_headers.insert("amz-sdk-invocation-id".to_string(), invocation_id.clone());
        signed_headers.insert("amz-sdk-request".to_string(), "attempt=1; max=3".to_string());
        signed_headers.insert("host".to_string(), host_header.clone());
        signed_headers.insert("x-amz-content-sha256".to_string(), payload_hash.clone());
        signed_headers.insert("x-amz-date".to_string(), amz_date.clone());
        signed_headers.insert(
            "x-amz-user-agent".to_string(),
            CHERRY_X_AMZ_USER_AGENT.to_string(),
        );

        let canonical_headers = signed_headers
            .iter()
            .map(|(name, value)| format!("{name}:{}\n", normalize_header_value(value)))
            .collect::<String>();
        let signed_header_names = signed_headers
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(";");
        let canonical_request = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            method.as_str(),
            path,
            query_string,
            canonical_headers,
            signed_header_names,
            payload_hash
        );
        let credential_scope = format!("{short_date}/{}/s3/aws4_request", self.config.region);
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{}\n{}\n{}",
            amz_date,
            credential_scope,
            sha256_hex(canonical_request.as_bytes())
        );
        let signature = hex::encode(sign_v4(
            &self.config.secret_access_key,
            &short_date,
            &self.config.region,
            "s3",
            string_to_sign.as_bytes(),
        ));
        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
            self.config.access_key_id, credential_scope, signed_header_names, signature
        );

        if s3_trace_enabled() {
            eprintln!("S3 TRACE request:");
            eprintln!("  method: {}", method);
            eprintln!("  url: {}", request_url);
            eprintln!("  host: {}", host_header);
            eprintln!("  canonical_request:\n{}", canonical_request);
            eprintln!("  string_to_sign:\n{}", string_to_sign);
            eprintln!("  authorization: {}", authorization);
        }

        let mut builder = Request::builder()
            .method(method)
            .uri(&request_url)
            .header("host", host_header)
            .header("user-agent", CHERRY_USER_AGENT)
            .header("x-amz-user-agent", CHERRY_X_AMZ_USER_AGENT)
            .header("amz-sdk-invocation-id", invocation_id)
            .header("amz-sdk-request", "attempt=1; max=3")
            .header("x-amz-date", amz_date)
            .header("x-amz-content-sha256", payload_hash)
            .header("authorization", authorization)
            .header("connection", "close");
        if let Some(content_type) = content_type {
            builder = builder.header("content-type", content_type);
        }

        let request = builder
            .body(Body::from(body))
            .context("failed to build raw HTTP request")?;
        let response = self
            .http
            .request(request)
            .await
            .map_err(|err| anyhow!("raw HTTP request failed: {err:?}"))?;
        let status = response.status();
        let body = to_bytes(response.into_body())
            .await
            .context("failed to read raw HTTP response body")?
            .to_vec();
        Ok(RawResponse { status, body })
    }
}

fn canonical_target(
    endpoint: &mut Url,
    bucket: &str,
    force_path_style: bool,
    key: Option<&str>,
) -> Result<(String, String)> {
    let host = endpoint
        .host_str()
        .ok_or_else(|| anyhow!("endpoint URL is missing host"))?;
    let mut host_header = host.to_string();
    if let Some(port) = endpoint.port() {
        let default_port =
            (endpoint.scheme() == "https" && port == 443) || (endpoint.scheme() == "http" && port == 80);
        if !default_port {
            host_header = format!("{host}:{port}");
        }
    }

    if !force_path_style {
        let bucket_host = format!("{bucket}.{host}");
        endpoint
            .set_host(Some(&bucket_host))
            .context("failed to set virtual-host style bucket host")?;
        host_header = if let Some(port) = endpoint.port() {
            format!("{bucket_host}:{port}")
        } else {
            bucket_host
        };
    }

    let path = if force_path_style {
        match key {
            Some(key) => format!("/{bucket}/{}", uri_encode(key, false)),
            None => format!("/{bucket}/"),
        }
    } else {
        match key {
            Some(key) => format!("/{}", uri_encode(key, false)),
            None => "/".to_string(),
        }
    };

    Ok((host_header, path))
}

fn canonical_query_string(query: &[(String, String)]) -> String {
    let mut params = query
        .iter()
        .map(|(key, value)| (uri_encode(key, true), uri_encode(value, true)))
        .collect::<Vec<_>>();
    params.sort();
    params
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

fn uri_encode(value: &str, encode_slash: bool) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match *byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'~' => encoded.push(char::from(*byte)),
            b'/' if !encode_slash => encoded.push('/'),
            _ => encoded.push_str(&format!("%{:02X}", byte)),
        }
    }
    encoded
}

fn normalize_header_value(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn sha256_hex(data: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(data.as_ref()))
}

fn sign_v4(secret: &str, short_date: &str, region: &str, service: &str, message: &[u8]) -> [u8; 32] {
    let k_date = hmac_sha256(format!("AWS4{secret}").as_bytes(), short_date.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    let k_signing = hmac_sha256(&k_service, b"aws4_request");
    hmac_sha256(&k_signing, message)
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut normalized_key = [0_u8; 64];
    if key.len() > 64 {
        let digest = Sha256::digest(key);
        normalized_key[..32].copy_from_slice(&digest);
    } else {
        normalized_key[..key.len()].copy_from_slice(key);
    }

    let mut ipad = [0x36_u8; 64];
    let mut opad = [0x5c_u8; 64];
    for index in 0..64 {
        ipad[index] ^= normalized_key[index];
        opad[index] ^= normalized_key[index];
    }

    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(message);
    let inner_digest = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_digest);
    let digest = outer.finalize();

    let mut out = [0_u8; 32];
    out.copy_from_slice(&digest);
    out
}

fn format_amz_date(time: OffsetDateTime) -> String {
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        time.year(),
        u8::from(time.month()),
        time.day(),
        time.hour(),
        time.minute(),
        time.second()
    )
}

fn format_short_date(time: OffsetDateTime) -> String {
    format!("{:04}{:02}{:02}", time.year(), u8::from(time.month()), time.day())
}

fn random_invocation_id() -> String {
    let random = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect::<String>()
        .to_ascii_lowercase();
    format!(
        "{}-{}-{}-{}-{}",
        &random[0..8],
        &random[8..12],
        &random[12..16],
        &random[16..20],
        &random[20..32]
    )
}

fn format_raw_http_error(operation: &str, response: &RawResponse) -> String {
    let mut lines = vec![
        format!("{operation} failed."),
        format!("  http_status: {}", response.status.as_u16()),
    ];
    if !response.body.is_empty() {
        lines.push(format!(
            "  response_body: {}",
            String::from_utf8_lossy(&response.body).trim()
        ));
    }
    lines.join("\n")
}

fn infer_force_path_style(endpoint: &str) -> bool {
    let Ok(url) = Url::parse(endpoint) else {
        return true;
    };

    let Some(host) = url.host_str() else {
        return true;
    };

    if host.eq_ignore_ascii_case("localhost") || host.parse::<IpAddr>().is_ok() {
        return true;
    }

    !VIRTUAL_HOST_SUFFIXES
        .iter()
        .any(|suffix| host.ends_with(suffix))
}

fn parse_list_object_keys(xml: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut remaining = xml;
    while let Some(start) = remaining.find("<Contents>") {
        remaining = &remaining[start + "<Contents>".len()..];
        let Some(end) = remaining.find("</Contents>") else {
            break;
        };
        let block = &remaining[..end];
        if let Some(key) = xml_tag(block, "Key") {
            keys.push(key.to_string());
        }
        remaining = &remaining[end + "</Contents>".len()..];
    }
    keys
}

fn s3_trace_enabled() -> bool {
    env::var_os("RUPASS_S3_TRACE").is_some()
}

fn normalize_manifest(mut manifest: Manifest) -> Manifest {
    if let Some(entry) = manifest.files.remove(LEGACY_TEAM_METADATA_FILE) {
        manifest.files.insert(TEAM_METADATA_FILE.to_string(), entry);
    }
    manifest
}

fn xml_tag<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let start_tag = format!("<{tag}>");
    let end_tag = format!("</{tag}>");
    let start = xml.find(&start_tag)? + start_tag.len();
    let end = xml[start..].find(&end_tag)? + start;
    Some(&xml[start..end])
}

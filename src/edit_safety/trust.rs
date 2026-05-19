use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

use crate::hash;

pub const TRUST_ENV_OVERRIDE: &str = "SYMFORGE_TRUST_PROJECT_CONFIG";
pub const CI_ENV_VARS: &[&str] = &[
    "CI",
    "GITHUB_ACTIONS",
    "GITLAB_CI",
    "BUILDKITE",
    "CIRCLECI",
    "JENKINS_URL",
    "TEAMCITY_VERSION",
    "TF_BUILD",
    "APPVEYOR",
    "DRONE",
];

const TRUST_STORE_SCHEMA_VERSION: u32 = 1;
const HASH_FRAME_PREFIX: &[u8] = b"symforge-project-config-v1\0";
const MAX_CONFIG_FILES: usize = 1024;
const MAX_CONFIG_FILE_BYTES: u64 = 1024 * 1024;
const MAX_CONFIG_TOTAL_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustStatus {
    Trusted,
    Untrusted,
    ContentChanged { expected: String, actual: String },
    EnvOverride,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustEvaluation {
    pub status: TrustStatus,
    pub project_key: Option<String>,
    pub actual_hash: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustRecord {
    pub project_key: String,
    pub trusted_hash: String,
    pub trusted_at: String,
    pub writer: String,
}

#[derive(Debug, Clone)]
pub struct ProjectConfigTrust {
    store_path: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum TrustStoreError {
    #[error("cannot determine canonical project key")]
    MissingProjectKey,
    #[error("refusing to record malformed project config hash")]
    InvalidHash,
    #[error("trust store schema version {0} is not supported")]
    UnsupportedSchemaVersion(u32),
    #[error("trust store is corrupt: {0}")]
    CorruptStore(String),
    #[error("trust store I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("trust store JSON serialization failed: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TrustStore {
    schema_version: u32,
    records: BTreeMap<String, TrustRecord>,
}

#[derive(Debug)]
struct ProjectConfigDigest {
    hash: String,
    warnings: Vec<String>,
}

#[derive(Debug)]
enum StoreLoadError {
    Missing,
    UnsupportedSchemaVersion(u32),
    Corrupt(String),
    Io(io::Error),
}

impl ProjectConfigTrust {
    pub fn default_store() -> Option<Self> {
        default_trust_store_path().map(Self::with_store_path)
    }

    pub fn with_store_path(store_path: impl AsRef<Path>) -> Self {
        Self {
            store_path: store_path.as_ref().to_path_buf(),
        }
    }

    pub fn store_path(&self) -> &Path {
        &self.store_path
    }

    pub fn evaluate(&self, project_root: impl AsRef<Path>) -> TrustEvaluation {
        let project_root = project_root.as_ref();
        let mut warnings = Vec::new();
        let digest = match hash_project_config(project_root) {
            Ok(digest) => digest,
            Err(message) => {
                return TrustEvaluation {
                    status: TrustStatus::Untrusted,
                    project_key: canonical_project_key(project_root).ok(),
                    actual_hash: String::new(),
                    warnings: vec![format!("{message}; project config trust is untrusted")],
                };
            }
        };
        let digest_has_warnings = !digest.warnings.is_empty();
        warnings.extend(digest.warnings);

        let project_key = match canonical_project_key(project_root) {
            Ok(project_key) => Some(project_key),
            Err(err) => {
                warnings.push(format!(
                    "could not canonicalize project path {}: {err}; project config is untrusted",
                    project_root.display()
                ));
                None
            }
        };

        if trust_env_override_requested() {
            if recognized_ci_environment() {
                return TrustEvaluation {
                    status: TrustStatus::EnvOverride,
                    project_key,
                    actual_hash: digest.hash,
                    warnings,
                };
            }
            warnings.push(format!(
                "{TRUST_ENV_OVERRIDE}=1 ignored because the process is not recognized as CI"
            ));
        }

        if digest_has_warnings {
            return TrustEvaluation {
                status: TrustStatus::Untrusted,
                project_key,
                actual_hash: digest.hash,
                warnings,
            };
        }

        let Some(project_key_value) = project_key.as_ref() else {
            return TrustEvaluation {
                status: TrustStatus::Untrusted,
                project_key,
                actual_hash: digest.hash,
                warnings,
            };
        };

        match self.load_store() {
            Ok(store) => match store.records.get(project_key_value) {
                Some(record) if !valid_sha256_hex(&record.trusted_hash) => {
                    warnings.push(
                        "trust record contains a malformed hash; project config is untrusted"
                            .to_string(),
                    );
                    TrustEvaluation {
                        status: TrustStatus::Untrusted,
                        project_key,
                        actual_hash: digest.hash,
                        warnings,
                    }
                }
                Some(record) if record.trusted_hash == digest.hash => TrustEvaluation {
                    status: TrustStatus::Trusted,
                    project_key,
                    actual_hash: digest.hash,
                    warnings,
                },
                Some(record) => TrustEvaluation {
                    status: TrustStatus::ContentChanged {
                        expected: record.trusted_hash.clone(),
                        actual: digest.hash.clone(),
                    },
                    project_key,
                    actual_hash: digest.hash,
                    warnings,
                },
                None => {
                    warnings.push(
                        "no trust record for canonical project key; project config is untrusted"
                            .to_string(),
                    );
                    TrustEvaluation {
                        status: TrustStatus::Untrusted,
                        project_key,
                        actual_hash: digest.hash,
                        warnings,
                    }
                }
            },
            Err(StoreLoadError::Missing) => {
                warnings.push(format!(
                    "trust store {} does not exist; project config is untrusted",
                    self.store_path.display()
                ));
                TrustEvaluation {
                    status: TrustStatus::Untrusted,
                    project_key,
                    actual_hash: digest.hash,
                    warnings,
                }
            }
            Err(StoreLoadError::UnsupportedSchemaVersion(version)) => {
                warnings.push(format!(
                    "trust store schema version {version} is unsupported; project config is untrusted"
                ));
                TrustEvaluation {
                    status: TrustStatus::Untrusted,
                    project_key,
                    actual_hash: digest.hash,
                    warnings,
                }
            }
            Err(StoreLoadError::Corrupt(message)) => {
                warnings.push(format!(
                    "trust store is corrupt: {message}; project config is untrusted"
                ));
                TrustEvaluation {
                    status: TrustStatus::Untrusted,
                    project_key,
                    actual_hash: digest.hash,
                    warnings,
                }
            }
            Err(StoreLoadError::Io(err)) => {
                warnings.push(format!(
                    "could not read trust store {}: {err}; project config is untrusted",
                    self.store_path.display()
                ));
                TrustEvaluation {
                    status: TrustStatus::Untrusted,
                    project_key,
                    actual_hash: digest.hash,
                    warnings,
                }
            }
        }
    }

    pub fn record_trust(
        &self,
        evaluation: &TrustEvaluation,
    ) -> Result<TrustRecord, TrustStoreError> {
        let project_key = evaluation
            .project_key
            .clone()
            .ok_or(TrustStoreError::MissingProjectKey)?;
        if !valid_sha256_hex(&evaluation.actual_hash) {
            return Err(TrustStoreError::InvalidHash);
        }

        let mut store = match self.load_store() {
            Ok(store) => store,
            Err(StoreLoadError::Missing) => TrustStore::default(),
            Err(StoreLoadError::UnsupportedSchemaVersion(version)) => {
                return Err(TrustStoreError::UnsupportedSchemaVersion(version));
            }
            Err(StoreLoadError::Corrupt(message)) => {
                return Err(TrustStoreError::CorruptStore(message));
            }
            Err(StoreLoadError::Io(err)) => return Err(TrustStoreError::Io(err)),
        };

        let record = TrustRecord {
            project_key: project_key.clone(),
            trusted_hash: evaluation.actual_hash.clone(),
            trusted_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
            writer: format!("symforge/{}", env!("CARGO_PKG_VERSION")),
        };
        store.records.insert(project_key, record.clone());
        self.write_store(&store)?;
        Ok(record)
    }

    fn load_store(&self) -> Result<TrustStore, StoreLoadError> {
        let contents = match fs::read_to_string(&self.store_path) {
            Ok(contents) => contents,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Err(StoreLoadError::Missing);
            }
            Err(err) => return Err(StoreLoadError::Io(err)),
        };
        let store: TrustStore = serde_json::from_str(&contents)
            .map_err(|err| StoreLoadError::Corrupt(err.to_string()))?;
        if store.schema_version != TRUST_STORE_SCHEMA_VERSION {
            return Err(StoreLoadError::UnsupportedSchemaVersion(
                store.schema_version,
            ));
        }
        Ok(store)
    }

    fn write_store(&self, store: &TrustStore) -> Result<(), TrustStoreError> {
        if let Some(parent) = self.store_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(store)?;
        fs::write(&self.store_path, json)?;
        Ok(())
    }
}

impl Default for TrustStore {
    fn default() -> Self {
        Self {
            schema_version: TRUST_STORE_SCHEMA_VERSION,
            records: BTreeMap::new(),
        }
    }
}

pub fn default_trust_store_path() -> Option<PathBuf> {
    dirs::data_local_dir().map(|dir| dir.join("symforge").join("trust.json"))
}

fn canonical_project_key(project_root: &Path) -> io::Result<String> {
    let canonical = dunce::canonicalize(project_root)?;
    Ok(canonical.to_string_lossy().into_owned())
}

fn trust_env_override_requested() -> bool {
    std::env::var(TRUST_ENV_OVERRIDE).is_ok_and(|value| value == "1")
}

fn recognized_ci_environment() -> bool {
    CI_ENV_VARS
        .iter()
        .any(|key| std::env::var_os(key).is_some_and(|value| !value.is_empty()))
}

fn valid_sha256_hex(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn hash_project_config(project_root: &Path) -> Result<ProjectConfigDigest, String> {
    let mut files = Vec::new();
    let mut warnings = Vec::new();
    let symforge_dir = project_root.join(".symforge");
    let config_toml = symforge_dir.join("config.toml");
    collect_optional_config_file(&config_toml, &mut files, &mut warnings)?;
    collect_config_dir(&symforge_dir.join("config"), &mut files, &mut warnings)?;
    files.sort_by_key(|path| relative_project_path(project_root, path));

    let mut frame = Vec::from(HASH_FRAME_PREFIX);
    let mut total_bytes = 0_u64;
    for path in files {
        let metadata = fs::symlink_metadata(&path)
            .map_err(|err| format!("could not inspect config file {}: {err}", path.display()))?;
        if metadata.len() > MAX_CONFIG_FILE_BYTES {
            return Err(format!(
                "config file {} is {} bytes, above {MAX_CONFIG_FILE_BYTES} byte cap",
                path.display(),
                metadata.len()
            ));
        }
        total_bytes = total_bytes
            .checked_add(metadata.len())
            .ok_or_else(|| "config byte count overflowed".to_string())?;
        if total_bytes > MAX_CONFIG_TOTAL_BYTES {
            return Err(format!(
                "project config is {total_bytes} bytes, above {MAX_CONFIG_TOTAL_BYTES} byte cap"
            ));
        }

        let bytes = fs::read(&path)
            .map_err(|err| format!("could not read config file {}: {err}", path.display()))?;
        let relative = relative_project_path(project_root, &path);
        frame.extend_from_slice(relative.as_bytes());
        frame.push(0);
        frame.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
        frame.push(0);
        frame.extend_from_slice(&bytes);
        frame.push(0);
    }

    Ok(ProjectConfigDigest {
        hash: hash::digest_hex(&frame),
        warnings,
    })
}

fn collect_optional_config_file(
    path: &Path,
    files: &mut Vec<PathBuf>,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => {
            return Err(format!(
                "could not inspect config file {}: {err}",
                path.display()
            ));
        }
    };
    let file_type = metadata.file_type();
    if file_type.is_file() {
        files.push(path.to_path_buf());
    } else {
        warnings.push(format!(
            "skipped non-file config path {}; project config is untrusted until fixed",
            path.display()
        ));
    }
    Ok(())
}

fn collect_config_dir(
    dir: &Path,
    files: &mut Vec<PathBuf>,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => {
            return Err(format!(
                "could not read config directory {}: {err}",
                dir.display()
            ));
        }
    };

    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry
            .map_err(|err| format!("could not read config directory {}: {err}", dir.display()))?;
        paths.push(entry.path());
    }
    paths.sort_by_key(|path| relative_project_path(dir, path));

    for path in paths {
        if files.len() >= MAX_CONFIG_FILES {
            return Err(format!(
                "project config has more than {MAX_CONFIG_FILES} files"
            ));
        }
        let metadata = fs::symlink_metadata(&path)
            .map_err(|err| format!("could not inspect config path {}: {err}", path.display()))?;
        let file_type = metadata.file_type();
        if file_type.is_file() {
            files.push(path);
        } else if file_type.is_dir() {
            collect_config_dir(&path, files, warnings)?;
        } else {
            warnings.push(format!(
                "skipped unsupported config path {}; project config is untrusted until fixed",
                path.display()
            ));
        }
    }
    Ok(())
}

fn relative_project_path(project_root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(project_root).unwrap_or(path);
    relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

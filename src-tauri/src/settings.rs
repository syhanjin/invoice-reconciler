use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::error::{AppError, AppResult};
use crate::models::{
    DEFAULT_INVOICE_OCR_CONCURRENCY, DEFAULT_MATCH_MAX_CANDIDATES, DEFAULT_MATCH_MAX_ORDERS,
    MAX_MATCH_MAX_CANDIDATES, MAX_MATCH_MAX_ORDERS, MIN_MATCH_MAX_CANDIDATES, MIN_MATCH_MAX_ORDERS,
    PrivateAppSettings, UpdateSettingsRequest,
};

const SETTINGS_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SettingsFile {
    version: u32,
    ollama: OllamaSettings,
    tencent_cloud: TencentSettings,
    #[serde(default)]
    matching: MatchingSettings,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct OllamaSettings {
    base_url: String,
    model: String,
    max_concurrency: u8,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct TencentSettings {
    secret_id: String,
    secret_key: String,
    invoice_ocr_concurrency: u8,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct MatchingSettings {
    max_orders_per_candidate: u8,
    max_candidates_per_invoice: u16,
}

impl Default for MatchingSettings {
    fn default() -> Self {
        Self {
            max_orders_per_candidate: DEFAULT_MATCH_MAX_ORDERS,
            max_candidates_per_invoice: DEFAULT_MATCH_MAX_CANDIDATES,
        }
    }
}

#[derive(Clone)]
pub struct SettingsStore {
    path: PathBuf,
    current: Arc<RwLock<PrivateAppSettings>>,
}

impl SettingsStore {
    pub async fn load_or_migrate(
        app_data_dir: &Path,
        legacy: crate::models::AppSettings,
    ) -> AppResult<Self> {
        tokio::fs::create_dir_all(app_data_dir).await?;
        let path = app_data_dir.join("settings.toml");
        let file = if path.exists() {
            let content = tokio::fs::read_to_string(&path).await.map_err(|error| {
                AppError::Settings(format!("无法读取 {}：{error}", path.display()))
            })?;
            let parsed: SettingsFile = toml::from_str(&content).map_err(|_| {
                AppError::Settings("设置格式无效，请检查 settings.toml 的字段和版本".into())
            })?;
            if parsed.version != SETTINGS_VERSION {
                return Err(AppError::Settings(format!(
                    "不支持的设置文件版本 {}",
                    parsed.version
                )));
            }
            parsed
        } else {
            let initial = SettingsFile {
                version: SETTINGS_VERSION,
                ollama: OllamaSettings {
                    base_url: legacy.ollama_base_url,
                    model: legacy.ollama_model,
                    max_concurrency: legacy.max_concurrency,
                },
                tencent_cloud: TencentSettings {
                    secret_id: String::new(),
                    secret_key: String::new(),
                    invoice_ocr_concurrency: DEFAULT_INVOICE_OCR_CONCURRENCY,
                },
                matching: MatchingSettings::default(),
            };
            write_file(&path, &initial).await?;
            initial
        };
        let warning = tighten_permissions(&path)
            .await
            .err()
            .map(|error| error.to_string());
        Ok(Self {
            path: path.clone(),
            current: Arc::new(RwLock::new(to_private(file, &path, warning))),
        })
    }

    pub async fn private(&self) -> PrivateAppSettings {
        self.current.read().await.clone()
    }

    pub async fn public(&self) -> crate::models::AppSettings {
        self.current.read().await.public()
    }

    pub async fn save(
        &self,
        request: UpdateSettingsRequest,
    ) -> AppResult<crate::models::AppSettings> {
        let validated = request.clone().validate()?;
        let mut current = self.current.write().await;
        let secret_id = if request.clear_tencent_credentials {
            String::new()
        } else {
            request
                .tencent_secret_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| current.tencent_secret_id.clone())
        };
        let secret_key = if request.clear_tencent_credentials {
            String::new()
        } else {
            request
                .tencent_secret_key
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| current.tencent_secret_key.clone())
        };
        if secret_id.is_empty() != secret_key.is_empty() {
            return Err(AppError::InvalidInput(
                "腾讯云 SecretId 和 SecretKey 必须同时填写".into(),
            ));
        }
        let file = SettingsFile {
            version: SETTINGS_VERSION,
            ollama: OllamaSettings {
                base_url: validated.ollama_base_url,
                model: validated.ollama_model,
                max_concurrency: validated.max_concurrency,
            },
            tencent_cloud: TencentSettings {
                secret_id,
                secret_key,
                invoice_ocr_concurrency: request.invoice_ocr_concurrency,
            },
            matching: MatchingSettings {
                max_orders_per_candidate: request.match_max_orders,
                max_candidates_per_invoice: request.match_max_candidates,
            },
        };
        write_file(&self.path, &file).await?;
        let warning = tighten_permissions(&self.path)
            .await
            .err()
            .map(|error| error.to_string());
        let next = to_private(file, &self.path, warning);
        let public = next.public();
        *current = next;
        Ok(public)
    }
}

fn to_private(file: SettingsFile, path: &Path, warning: Option<String>) -> PrivateAppSettings {
    PrivateAppSettings {
        ollama_base_url: file.ollama.base_url.trim_end_matches('/').to_owned(),
        ollama_model: file.ollama.model,
        max_concurrency: file.ollama.max_concurrency.clamp(1, 8),
        tencent_secret_id: file.tencent_cloud.secret_id,
        tencent_secret_key: file.tencent_cloud.secret_key,
        invoice_ocr_concurrency: file.tencent_cloud.invoice_ocr_concurrency.clamp(1, 5),
        match_max_orders: file
            .matching
            .max_orders_per_candidate
            .clamp(MIN_MATCH_MAX_ORDERS, MAX_MATCH_MAX_ORDERS),
        match_max_candidates: file
            .matching
            .max_candidates_per_invoice
            .clamp(MIN_MATCH_MAX_CANDIDATES, MAX_MATCH_MAX_CANDIDATES),
        settings_path: path.to_string_lossy().into_owned(),
        permission_warning: warning,
    }
}

async fn write_file(path: &Path, file: &SettingsFile) -> AppResult<()> {
    let content = toml::to_string_pretty(file)
        .map_err(|error| AppError::Settings(format!("无法序列化设置：{error}")))?;
    let temporary = path.with_extension(format!("toml.{}.tmp", uuid::Uuid::now_v7()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut temporary_file = options
        .open(&temporary)
        .map_err(|error| AppError::Settings(format!("无法创建临时设置文件：{error}")))?;
    temporary_file
        .write_all(content.as_bytes())
        .and_then(|_| temporary_file.sync_all())
        .map_err(|error| AppError::Settings(format!("无法写入临时设置文件：{error}")))?;
    drop(temporary_file);
    tighten_permissions(&temporary).await.ok();
    if let Err(error) = replace_file(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> AppResult<()> {
    std::fs::rename(source, destination)
        .map_err(|error| AppError::Settings(format!("无法替换设置文件：{error}")))
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> AppResult<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(AppError::Settings(format!(
            "无法替换设置文件：{}",
            std::io::Error::last_os_error()
        )))
    } else {
        Ok(())
    }
}

#[cfg(unix)]
async fn tighten_permissions(path: &Path) -> AppResult<()> {
    use std::os::unix::fs::PermissionsExt;
    let permissions = std::fs::Permissions::from_mode(0o600);
    tokio::fs::set_permissions(path, permissions)
        .await
        .map_err(|error| AppError::Settings(format!("无法将设置文件权限设为 0600：{error}")))
}

#[cfg(windows)]
async fn tighten_permissions(path: &Path) -> AppResult<()> {
    let username = std::env::var("USERNAME")
        .map_err(|error| AppError::Settings(format!("无法读取当前 Windows 用户名：{error}")))?;
    let output = tokio::process::Command::new("icacls")
        .arg(path)
        .args(["/inheritance:r", "/grant:r", &format!("{username}:F")])
        .output()
        .await
        .map_err(|error| AppError::Settings(format!("无法启动 icacls：{error}")))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(AppError::Settings("无法限制设置文件 ACL 为当前用户".into()))
    }
}

#[cfg(not(any(unix, windows)))]
async fn tighten_permissions(_path: &Path) -> AppResult<()> {
    Err(AppError::Settings(
        "当前平台不支持自动限制设置文件权限".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::AppSettings;

    fn legacy() -> AppSettings {
        AppSettings {
            ollama_base_url: "http://127.0.0.1:11434".into(),
            ollama_model: "qwen".into(),
            max_concurrency: 3,
            tencent_secret_id_masked: None,
            tencent_credentials_configured: false,
            invoice_ocr_concurrency: 3,
            match_max_orders: DEFAULT_MATCH_MAX_ORDERS,
            match_max_candidates: DEFAULT_MATCH_MAX_CANDIDATES,
            settings_path: String::new(),
            permission_warning: None,
        }
    }

    #[tokio::test]
    async fn migrates_and_never_exposes_secret_key() {
        let directory = tempfile::tempdir().unwrap();
        let store = SettingsStore::load_or_migrate(directory.path(), legacy())
            .await
            .unwrap();
        let saved = store
            .save(UpdateSettingsRequest {
                ollama_base_url: "http://localhost:11434".into(),
                ollama_model: "qwen".into(),
                max_concurrency: 2,
                tencent_secret_id: Some("AKID1234567890".into()),
                tencent_secret_key: Some("private-value".into()),
                clear_tencent_credentials: false,
                invoice_ocr_concurrency: 4,
                match_max_orders: 10,
                match_max_candidates: 100,
            })
            .await
            .unwrap();
        assert!(saved.tencent_credentials_configured);
        assert!(
            !serde_json::to_string(&saved)
                .unwrap()
                .contains("private-value")
        );
        assert!(
            tokio::fs::read_to_string(directory.path().join("settings.toml"))
                .await
                .unwrap()
                .contains("private-value")
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(directory.path().join("settings.toml"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[tokio::test]
    async fn malformed_settings_error_does_not_echo_secret_content() {
        let directory = tempfile::tempdir().unwrap();
        tokio::fs::write(
            directory.path().join("settings.toml"),
            "version = 1\n[tencent_cloud]\nsecret_key = \"SENSITIVE-SENTINEL\" trailing-data\n",
        )
        .await
        .unwrap();

        let error = match SettingsStore::load_or_migrate(directory.path(), legacy()).await {
            Ok(_) => panic!("malformed settings should fail"),
            Err(error) => error.to_string(),
        };
        assert!(!error.contains("SENSITIVE-SENTINEL"));
        assert!(!error.contains("secret_key"));
    }

    #[tokio::test]
    async fn version_one_settings_without_matching_section_use_defaults() {
        let directory = tempfile::tempdir().unwrap();
        tokio::fs::write(
            directory.path().join("settings.toml"),
            "version = 1\n[ollama]\nbase_url = \"http://localhost:11434\"\nmodel = \"qwen\"\nmax_concurrency = 3\n[tencent_cloud]\nsecret_id = \"\"\nsecret_key = \"\"\ninvoice_ocr_concurrency = 3\n",
        )
        .await
        .unwrap();

        let store = SettingsStore::load_or_migrate(directory.path(), legacy())
            .await
            .unwrap();
        let public = store.public().await;
        assert_eq!(public.match_max_orders, DEFAULT_MATCH_MAX_ORDERS);
        assert_eq!(public.match_max_candidates, DEFAULT_MATCH_MAX_CANDIDATES);
    }
}

use keyring::Entry;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelEntry>,
}

#[derive(Debug, Deserialize)]
struct ModelEntry {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
}

fn get_key_for_provider(provider: &str) -> Result<String, String> {
    let service = match provider {
        "claude" => "bambumate-claude-api",
        "openai" => "bambumate-openai-api",
        "kimi" => "bambumate-kimi-api",
        "openrouter" => "bambumate-openrouter-api",
        _ => return Err(format!("Unknown provider: {}", provider)),
    };
    let entry = Entry::new(service, "bambumate").map_err(|e| e.to_string())?;
    match entry.get_password() {
        Ok(key) => Ok(key),
        Err(keyring::Error::NoEntry) => {
            Err(format!("No API key configured for {}. Set it above first.", provider))
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Get the local MCP server URL from preferences, with a default fallback.
fn get_local_server_url(app: &AppHandle) -> String {
    let store = app.store("preferences.json").ok();
    store
        .and_then(|s| {
            s.get("local_mcp_url")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| "http://localhost:1234".to_string())
}

#[tauri::command]
pub async fn list_models(app: AppHandle, provider: String) -> Result<Vec<ModelInfo>, String> {
    info!("Fetching models for provider: {}", provider);

    if provider == "local" {
        return list_local_models(&app).await;
    }

    let api_key = get_key_for_provider(&provider)?;
    let client = reqwest::Client::new();

    let request = match provider.as_str() {
        "claude" => client
            .get("https://api.anthropic.com/v1/models")
            .header("x-api-key", &api_key)
            .header("anthropic-version", "2023-06-01"),
        "openai" => client
            .get("https://api.openai.com/v1/models")
            .header("Authorization", format!("Bearer {}", api_key)),
        "kimi" => client
            .get("https://api.moonshot.cn/v1/models")
            .header("Authorization", format!("Bearer {}", api_key)),
        "openrouter" => client
            .get("https://openrouter.ai/api/v1/models")
            .header("Authorization", format!("Bearer {}", api_key)),
        _ => return Err(format!("Unknown provider: {}", provider)),
    };

    let resp = request
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        warn!(
            "Models API error for {} ({}): {}",
            provider, status, body
        );
        return Err(format!("API error ({})", status));
    }

    let models: ModelsResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let mut result: Vec<ModelInfo> = models
        .data
        .into_iter()
        .map(|m| {
            let name = m
                .display_name
                .or(m.name)
                .unwrap_or_else(|| m.id.clone());
            ModelInfo { id: m.id, name }
        })
        .collect();

    result.sort_by(|a, b| a.id.cmp(&b.id));
    info!("Found {} models for {}", result.len(), provider);
    Ok(result)
}

/// List models from a local OpenAI-compatible server (LM Studio, Ollama, etc.).
async fn list_local_models(app: &AppHandle) -> Result<Vec<ModelInfo>, String> {
    let base_url = get_local_server_url(app);
    let url = format!("{}/v1/models", base_url.trim_end_matches('/'));
    info!("Fetching local models from: {}", url);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() || e.is_connect() {
                format!(
                    "Cannot connect to local server at {}. Is your local model server running?",
                    base_url
                )
            } else {
                format!("Request failed: {}", e)
            }
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        return Err(format!(
            "Local server returned error ({}). Check your server at {}",
            status, base_url
        ));
    }

    let models: ModelsResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let mut result: Vec<ModelInfo> = models
        .data
        .into_iter()
        .map(|m| {
            let name = m
                .display_name
                .or(m.name)
                .unwrap_or_else(|| m.id.clone());
            ModelInfo { id: m.id, name }
        })
        .collect();

    result.sort_by(|a, b| a.id.cmp(&b.id));
    info!("Found {} models from local server", result.len());
    Ok(result)
}

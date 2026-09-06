use serde::{Deserialize, Serialize};
use serde_json::Value;

/// VS Code Marketplace 搜索结果项
#[derive(Debug, Serialize, Deserialize)]
pub struct ExtensionInfo {
    pub extension_id: String,
    pub extension_name: String,
    pub display_name: String,
    pub publisher: String,
    pub short_description: String,
    pub icon: Option<String>,
}

/// 搜索 VS Code Marketplace
#[tauri::command]
pub async fn search_vscode_marketplace(query: String, sort_by: Option<u8>) -> Result<Vec<ExtensionInfo>, String> {
    let sort = sort_by.unwrap_or(0);
    let sort_str = match sort {
        1 => "UpdatedDate",
        2 => "Name",
        3 => "Publisher",
        4 => "InstallCount",
        5 => "WeightedRating",
        _ => "Relevance",
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP client init failed: {}", e))?;

    let url = "https://marketplace.visualstudio.com/_apis/public/gallery/extensionquery";

    // 构建搜索条件：filterType 8 = 目标类型(VS Code), filterType 10 = 搜索词
    let mut criteria = vec![
        serde_json::json!({"filterType": 8, "value": "Microsoft.VisualStudio.Code"}),
    ];

    // 只有非空查询才添加搜索词过滤
    if !query.is_empty() && query != "popular" {
        criteria.push(serde_json::json!({"filterType": 10, "value": query}));
    }

    let body = serde_json::json!({
        "filters": [{
            "criteria": criteria,
            "direction": 2,
            "pageSize": 30,
            "pageNumber": 1,
            "sortBy": sort_str,
            "sortOrder": 0
        }],
        "assetTypes": [],
        "flags": 950
    });

    eprintln!("[marketplace] Requesting: {} with query='{}' sort={}", url, query, sort_str);

    let response = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json;api-version=7.2-preview.1")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    let status = response.status();
    let raw_text = response.text().await.map_err(|e| format!("Read response failed: {}", e))?;

    eprintln!("[marketplace] Response status: {}, length: {}", status, raw_text.len());

    let data: Value = serde_json::from_str(&raw_text).map_err(|e| format!("JSON parse failed: {}", e))?;

    // 调试：打印响应结构
    if let Some(results) = data.get("results") {
        if let Some(arr) = results.as_array() {
            if let Some(first) = arr.first() {
                let ext_count = first.get("extensions").and_then(|e| e.as_array()).map(|a| a.len()).unwrap_or(0);
                eprintln!("[marketplace] Found {} extensions in response", ext_count);
            }
        }
    }

    let mut results = Vec::new();

    if let Some(extensions) = data
        .get("results")
        .and_then(|r| r.as_array())
        .and_then(|arr| arr.first())
        .and_then(|first| first.get("extensions"))
        .and_then(|e| e.as_array())
    {
        for ext in extensions {
            let extension_name = ext
                .get("extensionName")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let publisher_name = ext
                .get("publisher")
                .and_then(|p| p.get("publisherName"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let extension_id = format!("{}.{}", publisher_name, extension_name);
            let display_name = ext
                .get("displayName")
                .and_then(|v| v.as_str())
                .unwrap_or(&extension_name)
                .to_string();
            let short_description = ext
                .get("shortDescription")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let icon = ext
                .get("versions")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.first())
                .and_then(|first| first.get("files"))
                .and_then(|f| f.as_array())
                .and_then(|files| {
                    files.iter().find_map(|file| {
                        if file.get("assetType").and_then(|a| a.as_str()) == Some("Microsoft.VisualStudio.Services.Icons.Default") {
                            file.get("source").and_then(|s| s.as_str()).map(|s| s.to_string())
                        } else {
                            None
                        }
                    })
                });

            results.push(ExtensionInfo {
                extension_id,
                extension_name,
                display_name,
                publisher: publisher_name,
                short_description,
                icon,
            });
        }
    }

    eprintln!("[marketplace] Returning {} results", results.len());
    Ok(results)
}

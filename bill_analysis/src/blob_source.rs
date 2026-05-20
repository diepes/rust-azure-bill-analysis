//! Azure Blob Storage source for AmortizedCost billing exports.
//!
//! Configured via three environment variables:
//!   AZ_BILLING_BLOB_SERVICE_URL  e.g. https://eroadstaazurebilling.blob.core.windows.net/
//!   AZ_BILLING_CONTAINER_NAME    e.g. azurecostmanagement
//!   AZ_BILLING_BLOB_PREFIX       e.g. eroad/Cortex-amortized-cost
//!
//! Blob layout under the container:
//!   {prefix}/{YYYYMMDD-YYYYMMDD}/{run-id-guid}/manifest.json
//!   {prefix}/{YYYYMMDD-YYYYMMDD}/{run-id-guid}/part_0_0001.csv
//!   ...

use azure_core::http::Url;
use azure_identity::DeveloperToolsCredential;
use azure_storage_blob::{BlobContainerClient, BlobServiceClient, models::BlobContainerClientListBlobsOptions};
use futures::TryStreamExt;
use serde::Deserialize;
use std::error::Error;
use std::path::PathBuf;

use crate::bills::Bills;
use crate::cmd_parse::FilterOpts;

/// Configuration loaded from environment variables.
#[derive(Debug, Clone)]
pub struct BlobSourceConfig {
    pub blob_service_url: String,
    pub container_name: String,
    /// Blob prefix up to (but not including) the date-range segment,
    /// e.g. `eroad/Cortex-amortized-cost`
    pub prefix: String,
}

impl BlobSourceConfig {
    /// Returns `Some` when all three required env vars are present and non-empty.
    pub fn from_env() -> Option<Self> {
        let url = std::env::var("AZ_BILLING_BLOB_SERVICE_URL")
            .ok()
            .filter(|s| !s.is_empty())?;
        let container = std::env::var("AZ_BILLING_CONTAINER_NAME")
            .ok()
            .filter(|s| !s.is_empty())?;
        let prefix = std::env::var("AZ_BILLING_BLOB_PREFIX")
            .ok()
            .filter(|s| !s.is_empty())?;
        Some(Self {
            blob_service_url: url,
            container_name: container,
            prefix,
        })
    }
}

// ---------------------------------------------------------------------------
// Manifest JSON deserialization types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestBlob {
    blob_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestRunInfo {
    end_date: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlobManifest {
    blobs: Vec<ManifestBlob>,
    run_info: ManifestRunInfo,
}

// ---------------------------------------------------------------------------
// BlobSource
// ---------------------------------------------------------------------------

/// Azure Blob Storage client for AmortizedCost billing exports.
pub struct BlobSource {
    pub config: BlobSourceConfig,
    container_client: BlobContainerClient,
}

impl BlobSource {
    /// Constructs a `BlobSource` using `DeveloperToolsCredential` (chains `az login` /
    /// `azd login`). Works locally; production deployments should extend this with
    /// `ManagedIdentityCredential` / `WorkloadIdentityCredential` when needed.
    pub fn new(config: BlobSourceConfig) -> azure_core::Result<Self> {
        let service_url = Url::parse(&config.blob_service_url)?;
        let credential = DeveloperToolsCredential::new(None)?;
        let service_client = BlobServiceClient::new(service_url, Some(credential), None)?;
        let container_client = service_client.blob_container_client(&config.container_name);
        Ok(Self { config, container_client })
    }

    /// Lists all `YYYYMMDD-YYYYMMDD` date-range folder names available under the
    /// configured prefix by scanning for `manifest.json` blobs.
    ///
    /// Blob names are expected to look like:
    ///   `{prefix}/{date-range}/{run-id}/manifest.json`
    ///
    /// Returns a sorted, deduplicated list of date-range strings.
    pub async fn list_date_range_folders(&self) -> azure_core::Result<Vec<String>> {
        let list_prefix = format!("{}/", self.config.prefix);

        let options = BlobContainerClientListBlobsOptions {
            prefix: Some(list_prefix.clone()),
            ..Default::default()
        };
        let mut stream = self.container_client.list_blobs(Some(options))?;

        let mut date_ranges: Vec<String> = Vec::new();
        while let Some(blob) = stream.try_next().await? {
            let name = match blob.name {
                Some(n) => n,
                None => continue,
            };

            // Only care about manifest files to locate date-range folders.
            if !name.ends_with("/manifest.json") {
                continue;
            }

            // Strip "{prefix}/" to get "{date-range}/{run-id}/manifest.json".
            let relative = name
                .strip_prefix(&list_prefix)
                .unwrap_or(name.as_str());

            // The first path segment is the date-range folder.
            if let Some(date_range) = relative.split('/').next() {
                if is_date_range(date_range) && !date_ranges.contains(&date_range.to_string()) {
                    date_ranges.push(date_range.to_string());
                }
            }
        }

        date_ranges.sort();
        Ok(date_ranges)
    }

    /// Downloads the manifest for the billing month `YYYY-MM` by listing blobs
    /// with prefix `{prefix}/{YYYYMM}` and parsing the first `manifest.json` found.
    ///
    /// Returns an error if no manifest is found for the given month.
    async fn fetch_manifest_for_month(
        &self,
        year: u32,
        month: u32,
    ) -> Result<BlobManifest, Box<dyn Error + Send + Sync>> {
        // Build a prefix that matches the start of the date-range folder for this month,
        // e.g. `eroad/Cortex-amortized-cost/202605` matches `…/20260501-20260531/…`.
        let month_prefix = format!("{}/{}{:02}", self.config.prefix, year, month);
        let options = BlobContainerClientListBlobsOptions {
            prefix: Some(month_prefix.clone()),
            ..Default::default()
        };
        let mut stream = self.container_client.list_blobs(Some(options))?;

        while let Some(blob) = stream.try_next().await? {
            let name = match blob.name {
                Some(n) => n,
                None => continue,
            };
            if !name.ends_with("/manifest.json") {
                continue;
            }
            eprintln!("[blob] downloading manifest: {name}");
            let bytes = self.download_blob(&name).await?;
            let manifest: BlobManifest = serde_json::from_slice(&bytes)?;
            eprintln!(
                "[blob] manifest endDate={} blobs={}",
                manifest.run_info.end_date,
                manifest.blobs.len()
            );
            // Cache manifest to disk so it's visible alongside the CSVs.
            let local_manifest = self.local_path_for_blob(&name);
            if let Some(parent) = local_manifest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&local_manifest, &bytes)?;
            eprintln!("[blob] cached manifest → {}", local_manifest.display());
            return Ok(manifest);
        }

        Err(format!(
            "No manifest.json found in blob storage for {year}-{month:02} under prefix '{month_prefix}'"
        )
        .into())
    }

    /// Loads all billing part CSVs for the given month, using a local disk cache.
    ///
    /// The manifest is always fetched from blob (it is small and detects when the
    /// current-month export has been replaced).  Each CSV part is only downloaded
    /// if it is not already present on disk under `./blob/{container}/{blob_name}`.
    pub async fn load_bills_for_month(
        &self,
        year: u32,
        month: u32,
        filter_opts: &FilterOpts,
    ) -> Result<Bills, Box<dyn Error + Send + Sync>> {
        let manifest = self.fetch_manifest_for_month(year, month).await?;

        // Pass 1 — ensure every file listed in the manifest is on disk.
        for blob_info in &manifest.blobs {
            let local_path = self.local_path_for_blob(&blob_info.blob_name);
            if local_path.exists() {
                eprintln!("[blob] cache hit  → {}", local_path.display());
            } else {
                eprintln!("[blob] downloading: {}", blob_info.blob_name);
                let bytes = self.download_blob(&blob_info.blob_name).await?;
                eprintln!("[blob] writing {} bytes → {}", bytes.len(), local_path.display());
                if let Some(parent) = local_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&local_path, &bytes)?;
            }
        }

        // Pass 2 — parse CSV parts into Bills.
        let mut merged: Option<Bills> = None;
        for blob_info in &manifest.blobs {
            if !blob_info.blob_name.ends_with(".csv") {
                continue;
            }
            let local_path = self.local_path_for_blob(&blob_info.blob_name);
            let mut part = Bills::default();
            part.parse_csv(&local_path, filter_opts)
                .map_err(|e| -> Box<dyn Error + Send + Sync> { e.to_string().into() })?;
            eprintln!("[blob] parsed {} entries from {}", part.len(), local_path.display());
            match merged.as_mut() {
                None => merged = Some(part),
                Some(existing) => existing.extend_with(part),
            }
        }

        merged.ok_or_else(|| {
            format!("Manifest for {year}-{month:02} contained no CSV parts").into()
        })
    }

    /// Downloads a single blob by name and returns its bytes.
    async fn download_blob(&self, blob_name: &str) -> azure_core::Result<Vec<u8>> {
        let result = self
            .container_client
            .blob_client(blob_name)
            .download(None)
            .await?;
        let bytes = result.body.collect().await?;
        Ok(bytes.to_vec())
    }

    /// Maps a blob path (relative to the container) to a local cache path.
    ///
    /// e.g. blob `eroad/Cortex-amortized-cost/20260501-20260531/{run-id}/part_0_0001.csv`
    ///   → `./blob/azurecostmanagement/eroad/Cortex-amortized-cost/20260501-20260531/{run-id}/part_0_0001.csv`
    fn local_path_for_blob(&self, blob_name: &str) -> PathBuf {
        let mut path = PathBuf::from("blob");
        path.push(&self.config.container_name);
        for segment in blob_name.split('/') {
            if !segment.is_empty() {
                path.push(segment);
            }
        }
        path
    }
}

/// Returns `true` when `s` matches the `YYYYMMDD-YYYYMMDD` format (17 ASCII digits + dash).
fn is_date_range(s: &str) -> bool {
    s.len() == 17
        && s.as_bytes()[8] == b'-'
        && s[..8].bytes().all(|b| b.is_ascii_digit())
        && s[9..].bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::is_date_range;

    #[test]
    fn valid_date_range() {
        assert!(is_date_range("20240801-20240831"));
        assert!(is_date_range("20260501-20260531"));
    }

    #[test]
    fn invalid_date_range() {
        assert!(!is_date_range("202408010-20240831")); // too long
        assert!(!is_date_range("2024080-20240831")); // too short
        assert!(!is_date_range("20240801_20240831")); // wrong separator
        assert!(!is_date_range("7192093c-7f13-4753-a186-58c842877141")); // run-id guid
        assert!(!is_date_range("manifest.json"));
    }
}
